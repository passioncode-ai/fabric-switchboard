//! SB-131 (operator, 2026-10-15): «автодобавление всех репозиториев нужных в проект, когда там
//! агент работает». Which repositories agents change, read from the transcripts of recent
//! Claude Code sessions: a `Write`, `Edit`, `MultiEdit` or `NotebookEdit` names a file, and a
//! `Bash` command that commits, pushes, stages or creates a repository names its folder (`cd`,
//! `git -C`). Each piece of evidence is paired with the session's working folder (the entry's
//! `cwd`), which says whose project the work is for. Reading a repository never counts.
//!
//! SB-132: Codex sessions too. A Codex rollout (`<CODEX_HOME>/sessions/YYYY/MM/DD/rollout-*.jsonl`)
//! names its working folder in `turn_context` (and `session_meta`), its file changes in
//! `apply_patch` patches (`*** Update File: …`) and its commands in `exec_command` calls (`cmd`,
//! `workdir`) or, in older versions, `shell` function calls; the same rules apply to them.
//!
//! Only paths leave this module: the transcript text is read in memory, lines are filtered by
//! cheap byte checks before any parsing, and nothing of a prompt, a file's content or a command
//! is kept or logged. Like the limit scan (SB-49), a transcript is read only where it grew.
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

/// A first read, or growth past this, reads only the last this many bytes of a transcript.
const TAIL_BYTES: u64 = 512 * 1024;
/// Evidence pairs kept per pass, after folding edits in one folder into one (a file edit counts
/// as work in its folder). Far more than the folders a pass of agents' work touches; past it the
/// rest of that pass is not counted.
const MAX_EVIDENCE: usize = 4096;
/// A command longer than this is not read (a pasted script, not a `cd … && git commit`).
const MAX_COMMAND: usize = 16 * 1024;
/// Git subcommands that change a repository: work, not a look.
const GIT_WRITES: [&str; 11] = [
    "commit",
    "push",
    "add",
    "init",
    "merge",
    "rebase",
    "cherry-pick",
    "am",
    "mv",
    "rm",
    "worktree",
];

/// One piece of evidence: an agent working in `cwd` changed something at `target`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Worked {
    pub cwd: PathBuf,
    pub target: PathBuf,
}

struct Seen {
    created: Option<SystemTime>,
    modified: Option<SystemTime>,
    len: u64,
    /// The end of the last complete line read.
    scanned_to: u64,
    /// A Codex rollout's working folder as last read (its lines do not carry one each).
    cwd: Option<PathBuf>,
}

/// How far each recent transcript was read.
#[derive(Default)]
pub(crate) struct Activity {
    files: HashMap<PathBuf, Seen>,
    /// Bytes read since the cache was made, for the cost tests.
    pub(crate) read: u64,
}

impl Activity {
    /// The evidence appended to `files` since the last call. A transcript seen for the first
    /// time is read from its last TAIL_BYTES, so a session already running counts too.
    #[cfg(test)]
    pub(crate) fn collect(&mut self, files: &[PathBuf]) -> Vec<Worked> {
        self.collect_while(files, || false).unwrap_or_default()
    }

    /// As `collect`, but stops between transcripts when `stop` answers true. A stopped collection
    /// returns no partial evidence, so a shutdown cannot register only part of a pass.
    pub(crate) fn collect_while(
        &mut self,
        files: &[PathBuf],
        mut stop: impl FnMut() -> bool,
    ) -> Option<Vec<Worked>> {
        let wanted: HashSet<&PathBuf> = files.iter().collect();
        self.files.retain(|path, _| wanted.contains(path));
        let mut found: Vec<Worked> = Vec::new();
        let mut unique: HashSet<Worked> = HashSet::new();
        for path in files {
            if stop() {
                return None;
            }
            if found.len() >= MAX_EVIDENCE {
                break;
            }
            for w in self.scan(path) {
                if found.len() < MAX_EVIDENCE && unique.insert(w.clone()) {
                    found.push(w);
                }
            }
        }
        Some(found)
    }

    fn scan(&mut self, path: &PathBuf) -> Vec<Worked> {
        let Ok(meta) = fs::metadata(path) else {
            return vec![];
        };
        let (len, created, modified) = (meta.len(), meta.created().ok(), meta.modified().ok());
        let from = match self.files.get(path) {
            Some(old)
                if old.created == created
                    && len >= old.scanned_to
                    && modified.is_none_or(|m| old.modified.is_none_or(|o| m >= o)) =>
            {
                if old.len == len && old.modified == modified {
                    return vec![];
                }
                Some(old.scanned_to)
            }
            _ => None,
        };
        let start = match from {
            Some(at) if len - at <= TAIL_BYTES => at,
            _ => len.saturating_sub(TAIL_BYTES),
        };
        let Some((bytes, end)) = read_lines(path, start, len) else {
            return vec![];
        };
        self.read += end.saturating_sub(start);
        // A read that does not start where the last one stopped begins mid-line.
        let body: &[u8] = if start > 0 && from != Some(start) {
            match bytes.iter().position(|b| *b == b'\n') {
                Some(cut) => &bytes[cut + 1..],
                None => &[],
            }
        } else {
            &bytes
        };
        let codex = is_rollout(path);
        let mut cwd = self.files.get(path).and_then(|s| s.cwd.clone());
        if codex && cwd.is_none() {
            // A first read from the tail may hold no `turn_context`: the session's start does.
            cwd = rollout_start_cwd(path);
        }
        let found = if codex {
            body.split(|b| *b == b'\n')
                .flat_map(|line| codex_evidence(line, &mut cwd))
                .collect()
        } else {
            body.split(|b| *b == b'\n').flat_map(evidence).collect()
        };
        self.files.insert(
            path.clone(),
            Seen {
                created,
                modified,
                len,
                scanned_to: end,
                cwd,
            },
        );
        found
    }
}

/// The complete lines between `start` and `len`, and the offset just past the last of them; a
/// line still being written is read again with the next growth.
fn read_lines(path: &Path, start: u64, len: u64) -> Option<(Vec<u8>, u64)> {
    let mut file = fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(len - start).read_to_end(&mut bytes).ok()?;
    let last = bytes.iter().rposition(|b| *b == b'\n')?;
    bytes.truncate(last);
    Some((bytes, start + last as u64 + 1))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// The evidence in one transcript line: none unless it is an assistant entry with a tool call
/// that changes something, and a `cwd` that is an absolute path.
pub(crate) fn evidence(line: &[u8]) -> Vec<Worked> {
    // Cheap checks first: most lines are never parsed.
    if !contains(line, b"\"tool_use\"")
        || !(contains(line, b"\"file_path\"")
            || contains(line, b"\"notebook_path\"")
            || contains(line, b"git "))
    {
        return vec![];
    }
    let Ok(entry) = serde_json::from_slice::<serde_json::Value>(line) else {
        return vec![];
    };
    if entry["type"] != "assistant" {
        return vec![];
    }
    let Some(cwd) = entry["cwd"].as_str().map(PathBuf::from) else {
        return vec![];
    };
    if !cwd.is_absolute() {
        return vec![];
    }
    let mut found = Vec::new();
    for call in entry["message"]["content"].as_array().into_iter().flatten() {
        if call["type"] != "tool_use" {
            continue;
        }
        let input = &call["input"];
        let targets: Vec<PathBuf> = match call["name"].as_str() {
            Some("Write" | "Edit" | "MultiEdit" | "NotebookEdit") => input["file_path"]
                .as_str()
                .or_else(|| input["notebook_path"].as_str())
                .and_then(|p| resolve(p, &cwd))
                // Work in a file is work in its folder: many edits fold into one.
                .and_then(|p| p.parent().map(Path::to_path_buf))
                .into_iter()
                .collect(),
            Some("Bash") => input["command"]
                .as_str()
                .filter(|c| c.len() <= MAX_COMMAND)
                .map(|c| git_targets(c, &cwd))
                .unwrap_or_default(),
            _ => vec![],
        };
        found.extend(targets.into_iter().map(|target| Worked {
            cwd: cwd.clone(),
            target,
        }));
    }
    found
}

/// A Codex rollout, by its file name.
fn is_rollout(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
}

/// The first line of a rollout (`session_meta`) carries the folder the session started in; it
/// can be long (the base instructions), so up to ROLLOUT_HEAD bytes are read for it.
const ROLLOUT_HEAD: u64 = 512 * 1024;
fn rollout_start_cwd(path: &Path) -> Option<PathBuf> {
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(ROLLOUT_HEAD).read_to_end(&mut bytes).ok()?;
    let line = &bytes[..bytes.iter().position(|b| *b == b'\n')?];
    let mut cwd = None;
    codex_evidence(line, &mut cwd);
    cwd
}

/// The evidence in one rollout line. `turn_context` and `session_meta` lines move `cwd`; tool
/// calls that change something are paired with it.
pub(crate) fn codex_evidence(line: &[u8], cwd: &mut Option<PathBuf>) -> Vec<Worked> {
    let context = contains(line, b"\"turn_context\"") || contains(line, b"\"session_meta\"");
    if !context
        && !(contains(line, b"\"response_item\"")
            && (contains(line, b"Begin Patch") || contains(line, b"git ")))
    {
        return vec![];
    }
    let Ok(entry) = serde_json::from_slice::<serde_json::Value>(line) else {
        return vec![];
    };
    let payload = &entry["payload"];
    if entry["type"] == "turn_context" || entry["type"] == "session_meta" {
        if let Some(dir) = payload["cwd"].as_str().map(PathBuf::from) {
            if dir.is_absolute() {
                *cwd = Some(dir);
            }
        }
        return vec![];
    }
    let Some(here) = cwd.clone() else {
        return vec![];
    };
    let mut targets: Vec<PathBuf> = Vec::new();
    match (payload["type"].as_str(), payload["name"].as_str()) {
        // Code mode: a script calling `tools.exec_command({cmd, workdir})` and `tools.apply_patch`.
        (Some("custom_tool_call"), Some("exec")) => {
            if let Some(script) = payload["input"].as_str().filter(|s| s.len() <= MAX_SCRIPT) {
                targets.extend(patch_targets(script, &here));
                for (command, workdir) in exec_commands(script) {
                    let dir = workdir
                        .as_deref()
                        .and_then(|w| resolve(w, &here))
                        .unwrap_or_else(|| here.clone());
                    targets.extend(git_targets(&command, &dir));
                }
            }
        }
        (Some("custom_tool_call"), Some("apply_patch")) => {
            if let Some(patch) = payload["input"].as_str().filter(|s| s.len() <= MAX_SCRIPT) {
                targets.extend(patch_targets(patch, &here));
            }
        }
        // Older versions: a `shell` function call with JSON arguments.
        (
            Some("function_call"),
            Some("shell" | "shell_command" | "exec_command" | "local_shell"),
        ) => {
            let args: serde_json::Value = payload["arguments"]
                .as_str()
                .and_then(|a| serde_json::from_str(a).ok())
                .unwrap_or_default();
            let command = match (&args["command"], &args["cmd"]) {
                (serde_json::Value::Array(words), _) => {
                    words.last().and_then(|w| w.as_str()).map(str::to_owned)
                }
                (serde_json::Value::String(c), _) | (_, serde_json::Value::String(c)) => {
                    Some(c.clone())
                }
                _ => None,
            };
            if let Some(command) = command.filter(|c| c.len() <= MAX_COMMAND) {
                let dir = args["workdir"]
                    .as_str()
                    .and_then(|w| resolve(w, &here))
                    .unwrap_or_else(|| here.clone());
                targets.extend(git_targets(&command, &dir));
            }
        }
        _ => {}
    }
    let mut seen = HashSet::new();
    targets.retain(|t| seen.insert(t.clone()));
    targets
        .into_iter()
        .map(|target| Worked {
            cwd: here.clone(),
            target,
        })
        .collect()
}

/// A Codex script longer than this is not read.
const MAX_SCRIPT: usize = 256 * 1024;

/// The folders an `apply_patch` patch changes: each file it adds, updates, deletes or moves to,
/// as its folder. The patch may sit in a script as text with real line breaks or as `\n` escapes.
fn patch_targets(text: &str, cwd: &Path) -> Vec<PathBuf> {
    const HEADS: [&str; 4] = [
        "*** Update File: ",
        "*** Add File: ",
        "*** Delete File: ",
        "*** Move to: ",
    ];
    let mut out = Vec::new();
    for line in text.split('\n').flat_map(|l| l.split("\\n")) {
        let line = line.trim_start();
        let Some(raw) = HEADS.iter().find_map(|h| line.strip_prefix(h)) else {
            continue;
        };
        let raw = raw.trim().trim_end_matches(['"', '\'', '`', ',', ';']);
        if let Some(folder) = resolve(raw, cwd).and_then(|p| p.parent().map(Path::to_path_buf)) {
            if !out.contains(&folder) {
                out.push(folder);
            }
        }
    }
    out
}

/// Each `exec_command({cmd: …, workdir: …})` in a Codex script: its command and folder, when both
/// are plain string literals. A template with `${…}` is decided at run time and skipped.
fn exec_commands(script: &str) -> Vec<(String, Option<String>)> {
    let calls: Vec<usize> = script
        .match_indices("exec_command(")
        .map(|(i, _)| i)
        .collect();
    let mut out = Vec::new();
    for (n, start) in calls.iter().enumerate() {
        let end = calls.get(n + 1).copied().unwrap_or(script.len());
        let call = &script[*start..end];
        let Some(command) = js_value(call, "cmd").filter(|c| c.len() <= MAX_COMMAND) else {
            continue;
        };
        out.push((command, js_value(call, "workdir")));
    }
    out
}

/// The string literal after `key:` in a piece of script ('…', "…" or `…` with escapes).
fn js_value(text: &str, key: &str) -> Option<String> {
    let mut from = 0;
    while let Some(at) = text[from..].find(key).map(|i| i + from) {
        from = at + key.len();
        let before = text[..at].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rest = text[from..].trim_start();
        let Some(rest) = rest.strip_prefix(':') else {
            continue;
        };
        return js_string(rest.trim_start());
    }
    None
}

fn js_string(text: &str) -> Option<String> {
    let mut chars = text.chars();
    let quote = chars.next().filter(|q| matches!(q, '"' | '\'' | '`'))?;
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                other => out.push(other),
            },
            '$' if quote == '`' && chars.clone().next() == Some('{') => return None,
            c if c == quote => return Some(out),
            c => out.push(c),
        }
    }
    None
}

/// The folders a shell command changes with git: each `git <write>` in it, in the folder its
/// `cd` and `-C` lead to. Separators inside quotes and heredoc bodies (a commit message, a pull
/// request text) are text, not commands, and `git` counts only as a command's first word.
/// Anything the shell would expand at run time is skipped, never guessed.
pub(crate) fn git_targets(command: &str, cwd: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut dir = cwd.to_owned();
    // A `cd` inside `( … )` or `$( … )` ends with it.
    let mut outer: Vec<PathBuf> = Vec::new();
    for words in commands(command) {
        if words.len() == 1 && words[0] == OPEN {
            outer.push(dir.clone());
            continue;
        }
        if words.len() == 1 && words[0] == CLOSE {
            if let Some(back) = outer.pop() {
                dir = back;
            }
            continue;
        }
        // `VAR=value git …`, `env … git …`, `command git …`: the command is the first plain word.
        let start = words
            .iter()
            .position(|w| (w.starts_with('-') || !w.contains('=')) && w != "command" && w != "env")
            .unwrap_or(words.len());
        let words = &words[start..];
        let Some(first) = words.first() else {
            continue;
        };
        if first == "cd" {
            match words.get(1).and_then(|w| resolve(w, &dir)) {
                Some(next) => dir = next,
                // `cd`, `cd -`, `cd $X`: where the rest of the command runs is unknown.
                None => return found,
            }
            continue;
        }
        if first != "git" && !first.ends_with("/git") {
            continue;
        }
        let mut here = dir.clone();
        let mut i = 1;
        while let Some(w) = words.get(i) {
            match w.as_str() {
                "-C" => {
                    match words.get(i + 1).and_then(|p| resolve(p, &here)) {
                        Some(next) => here = next,
                        None => break,
                    }
                    i += 2;
                }
                "-c" | "--git-dir" | "--work-tree" | "--namespace" => i += 2,
                w if w.starts_with('-') => i += 1,
                _ => break,
            }
        }
        let Some(verb) = words.get(i) else {
            continue;
        };
        if !GIT_WRITES.contains(&verb.as_str()) {
            continue;
        }
        if verb == "worktree" && words.get(i + 1).map(String::as_str) != Some("add") {
            continue;
        }
        if verb == "init" {
            // `git init <dir>` makes the repository there.
            if let Some(target) = words[i + 1..]
                .iter()
                .find(|w| !w.starts_with('-'))
                .and_then(|w| resolve(w, &here))
            {
                here = target;
            }
        }
        found.push(here);
    }
    found
}

/// Markers `commands` puts where a subshell opens and closes (never a word a shell would see).
const OPEN: &str = "\u{0}(";
const CLOSE: &str = "\u{0})";

/// What the next finished word is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Next {
    Word,
    /// The end word of a heredoc (`<<EOF`): its body is skipped.
    HeredocEnd,
    /// The target of a redirection (`> file`, `2>&1`): not a word of the command.
    Redirect,
}

/// A shell command line split into simple commands, each as its words: quotes removed and
/// respected (a `;` or `&&` inside quotes is text), grouping and redirections dropped, and the
/// body of every heredoc (`<<WORD`, `<<'WORD'`, `<<-WORD`) skipped to its end line. Good enough for
/// `cd "/a b" && git -C x commit -F - <<'EOF' … EOF`; not a shell.
fn commands(command: &str) -> Vec<Vec<String>> {
    let chars: Vec<char> = command.chars().collect();
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut next = Next::Word;
    let mut heredocs: Vec<String> = Vec::new();
    let finish = |word: &mut String,
                  started: &mut bool,
                  next: &mut Next,
                  words: &mut Vec<String>,
                  heredocs: &mut Vec<String>| {
        if !*started {
            return;
        }
        let w = std::mem::take(word);
        *started = false;
        match std::mem::replace(next, Next::Word) {
            Next::Word => words.push(w),
            Next::HeredocEnd => heredocs.push(w),
            Next::Redirect => {}
        }
    };
    let mut nested = 0usize;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if nested > 0 {
            // Inside `"$( … )"`: a command substitution, text for this purpose, whose quotes
            // (a commit message's) do not end the outer quote.
            match c {
                '(' => nested += 1,
                ')' => nested -= 1,
                _ => {}
            }
            word.push(c);
            i += 1;
            continue;
        }
        if let Some(q) = quote {
            if q == '"' && c == '$' && chars.get(i + 1) == Some(&'(') {
                nested = 1;
                word.push('$');
                word.push('(');
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            } else if q == '"' && c == '\\' && i + 1 < chars.len() {
                i += 1;
                word.push(chars[i]);
            } else {
                word.push(c);
            }
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' => {
                quote = Some(c);
                started = true;
            }
            '\\' => {
                i += 1;
                if let Some(&n) = chars.get(i) {
                    if n != '\n' {
                        word.push(n);
                        started = true;
                    }
                }
            }
            '<' if chars.get(i + 1) == Some(&'<') && chars.get(i + 2) != Some(&'<') => {
                finish(
                    &mut word,
                    &mut started,
                    &mut next,
                    &mut words,
                    &mut heredocs,
                );
                i += 1;
                if chars.get(i + 1) == Some(&'-') {
                    i += 1;
                }
                next = Next::HeredocEnd;
            }
            '<' | '>' => {
                // `2>&1`: the descriptor before the arrow is not a word either.
                if started && word.chars().all(|d| d.is_ascii_digit()) {
                    word.clear();
                    started = false;
                }
                finish(
                    &mut word,
                    &mut started,
                    &mut next,
                    &mut words,
                    &mut heredocs,
                );
                while chars
                    .get(i + 1)
                    .is_some_and(|n| matches!(n, '>' | '&' | '|' | '<'))
                {
                    i += 1;
                }
                next = Next::Redirect;
            }
            '\n' | ';' | '&' | '|' | '(' | ')' | '{' | '}' => {
                finish(
                    &mut word,
                    &mut started,
                    &mut next,
                    &mut words,
                    &mut heredocs,
                );
                next = Next::Word;
                if !words.is_empty() {
                    out.push(std::mem::take(&mut words));
                }
                if c == '(' {
                    out.push(vec![OPEN.to_owned()]);
                } else if c == ')' {
                    out.push(vec![CLOSE.to_owned()]);
                }
                if c == '\n' {
                    // Each heredoc started on this line: skip its body to its end line.
                    for end in std::mem::take(&mut heredocs) {
                        loop {
                            let from = i + 1;
                            if from >= chars.len() {
                                i = chars.len();
                                break;
                            }
                            let to = chars[from..]
                                .iter()
                                .position(|n| *n == '\n')
                                .map_or(chars.len(), |p| from + p);
                            let line: String = chars[from..to].iter().collect();
                            i = to;
                            if line.trim() == end {
                                break;
                            }
                        }
                    }
                }
            }
            c if c.is_whitespace() => finish(
                &mut word,
                &mut started,
                &mut next,
                &mut words,
                &mut heredocs,
            ),
            c => {
                word.push(c);
                started = true;
            }
        }
        i += 1;
    }
    finish(
        &mut word,
        &mut started,
        &mut next,
        &mut words,
        &mut heredocs,
    );
    if !words.is_empty() {
        out.push(words);
    }
    out
}

/// A path as the shell would see it from `dir`: `~` and `$HOME` expanded, relative paths joined,
/// `.` and `..` folded. None when it holds anything else the shell would expand.
pub(crate) fn resolve(raw: &str, dir: &Path) -> Option<PathBuf> {
    let home = || {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
    };
    let raw = raw.trim_end_matches(')');
    // A Windows verbatim path (`\\?\C:\…`, what `canonicalize` gives there) is a plain path:
    // its `?` is no glob.
    let raw = raw.strip_prefix(r"\\?\").unwrap_or(raw);
    let path = if raw == "~" || raw == "$HOME" || raw == "${HOME}" {
        home()?
    } else if let Some(rest) = raw
        .strip_prefix("~/")
        .or_else(|| raw.strip_prefix("$HOME/"))
        .or_else(|| raw.strip_prefix("${HOME}/"))
    {
        home()?.join(rest)
    } else {
        PathBuf::from(raw)
    };
    let text = path.to_string_lossy();
    if raw.is_empty() || text.contains(['$', '*', '?', '`', '{', '[']) || raw.starts_with('-') {
        return None;
    }
    let joined = if path.is_absolute() {
        path
    } else {
        dir.join(path)
    };
    let mut out = PathBuf::new();
    for part in joined.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    Some(out)
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    #[test]
    fn a_verbatim_windows_path_is_a_path() {
        // SB-131: canonical paths on Windows start with `\\?\`; the nightly Windows run found it.
        assert_eq!(
            resolve(r"\\?\C:\work\lib\src\a.rs", Path::new(r"C:\work")),
            Some(PathBuf::from(r"C:\work\lib\src\a.rs"))
        );
        assert_eq!(resolve(r"C:\work\*.rs", Path::new(r"C:\work")), None);
    }
}

// The tests use Unix paths; the parser itself is the same everywhere.
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn line(cwd: &str, calls: serde_json::Value) -> Vec<u8> {
        let mut text = serde_json::to_vec(&serde_json::json!({
            "type": "assistant", "cwd": cwd,
            "message": {"content": calls}
        }))
        .unwrap();
        text.push(b'\n');
        text
    }
    fn tool(name: &str, input: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"type": "tool_use", "name": name, "input": input})
    }
    fn targets(command: &str) -> Vec<String> {
        git_targets(command, Path::new("/w/app"))
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn a_shell_command_names_the_folders_it_commits_in_and_nothing_it_only_reads() {
        assert_eq!(targets("git commit -m x"), ["/w/app"]);
        assert_eq!(
            targets("cd /w/lib && git add -A && git commit -qm \"a b\""),
            ["/w/lib", "/w/lib"]
        );
        assert_eq!(targets("git -C ../lib push origin main"), ["/w/lib"]);
        assert_eq!(targets("cd '/w/my repo'; git push"), ["/w/my repo"]);
        assert_eq!(targets("(cd /w/x && git commit -am y)"), ["/w/x"]);
        assert_eq!(targets("mkdir -p /w/new && git init /w/new"), ["/w/new"]);
        assert_eq!(targets("git worktree add .worktrees/a -b a"), ["/w/app"]);
        assert_eq!(
            targets("GIT_EDITOR=true git -c user.name=x commit"),
            ["/w/app"]
        );
        // Reads, and commands whose folder the shell decides at run time, are not work.
        assert!(targets("cd /w/lib && git log --oneline | head").is_empty());
        assert!(targets("git -C /w/lib status && git diff").is_empty());
        assert!(targets("git worktree list").is_empty());
        assert!(targets("cd $DIR && git commit -m x").is_empty());
        assert!(targets("cd /w/a && cd - && git push").is_empty());
        assert!(targets("grep -r 'git commit' docs").is_empty());
        // Text is not a command: a commit message, a heredoc body, an echo.
        assert_eq!(
            targets("git add -A && git commit -q -F - <<'EOF'\nfix: x\n\ncd ../org && git add a && git commit -m row\nEOF\ngit push"),
            ["/w/app", "/w/app", "/w/app"]
        );
        assert_eq!(
            targets("git commit -m \"wip; cd /etc\" && git push"),
            ["/w/app", "/w/app"]
        );
        assert!(targets("echo run git add later").is_empty());
        assert_eq!(
            targets("git -C sub/lib push 2>&1 | tail -3"),
            ["/w/app/sub/lib"]
        );
        assert_eq!(targets("cd /w/x >/dev/null && git push"), ["/w/x"]);
        assert_eq!(targets("env GIT_X=1 git commit -m y"), ["/w/app"]);
        assert!(targets("cd && git commit -m x").is_empty());
        // A here-string, a commit message written with `"$(cat <<'EOF' … )"` and a `cd` inside a
        // subshell (SB-131 final review).
        assert_eq!(targets("cat <<< hi\ncd /w/b && git push"), ["/w/b"]);
        assert_eq!(
            targets(
                "git commit -m \"$(cat <<'EOF'\nfix 12\" screen\ncd /etc && git push\nEOF\n)\""
            ),
            ["/w/app"]
        );
        assert_eq!(targets("(cd /w/b && ls); git commit -m x"), ["/w/app"]);
        assert_eq!(
            targets("(cd /w/b && git push); git push"),
            ["/w/b", "/w/app"]
        );
        assert_eq!(
            resolve("../b/./c", Path::new("/w/a")),
            Some(PathBuf::from("/w/b/c"))
        );
        assert_eq!(resolve("*.rs", Path::new("/w")), None);
    }

    #[test]
    fn edits_and_commits_count_with_the_sessions_folder_and_reads_do_not() {
        let edit = line(
            "/w/app",
            serde_json::json!([
                tool(
                    "Edit",
                    serde_json::json!({"file_path": "/w/lib/src/a.rs", "old_string": "x", "new_string": "y"})
                ),
                tool(
                    "Read",
                    serde_json::json!({"file_path": "/w/other/README.md"})
                ),
                tool(
                    "Write",
                    serde_json::json!({"file_path": "notes/b.md", "content": "git commit"})
                ),
                tool(
                    "Bash",
                    serde_json::json!({"command": "cd /w/lib && git commit -m done"})
                ),
                tool(
                    "Bash",
                    serde_json::json!({"command": "git -C /w/other log"})
                ),
            ]),
        );
        let found: Vec<(String, String)> = evidence(edit.trim_ascii_end())
            .into_iter()
            .map(|w| {
                (
                    w.cwd.to_string_lossy().into_owned(),
                    w.target.to_string_lossy().into_owned(),
                )
            })
            .collect();
        assert_eq!(
            found,
            [
                ("/w/app".into(), "/w/lib/src".into()),
                ("/w/app".into(), "/w/app/notes".into()),
                ("/w/app".into(), "/w/lib".into()),
            ]
        );
        // A user's message, a tool result and an entry without an absolute `cwd` say nothing.
        let user = serde_json::to_vec(&serde_json::json!({"type": "user", "cwd": "/w/app",
            "message": {"content": [tool("Edit", serde_json::json!({"file_path": "/w/x/a"}))]}}))
        .unwrap();
        assert!(evidence(&user).is_empty());
        let relative = line(
            "w/app",
            serde_json::json!([tool("Edit", serde_json::json!({"file_path": "/w/x/a"}))]),
        );
        assert!(evidence(relative.trim_ascii_end()).is_empty());
        assert!(evidence(b"not json \"tool_use\" \"file_path\"").is_empty());
    }

    #[test]
    fn a_transcript_is_read_only_where_it_grew_and_a_line_being_written_waits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        let quiet = serde_json::to_vec(&serde_json::json!({"type": "user", "cwd": "/w/app",
            "message": {"content": "hello"}}))
        .unwrap();
        let mut text = Vec::new();
        for _ in 0..50 {
            text.extend_from_slice(&quiet);
            text.push(b'\n');
        }
        text.extend(line(
            "/w/app",
            serde_json::json!([tool("Edit", serde_json::json!({"file_path": "/w/lib/a"}))]),
        ));
        fs::write(&path, &text).unwrap();
        let mut activity = Activity::default();
        let files = vec![path.clone()];
        assert_eq!(activity.collect(&files).len(), 1);
        let after_first = activity.read;
        // Unchanged: nothing read, nothing found again.
        assert!(activity.collect(&files).is_empty());
        assert_eq!(activity.read, after_first);
        // A line still being written is not read until it ends.
        let next = line(
            "/w/app",
            serde_json::json!([tool(
                "Bash",
                serde_json::json!({"command": "git -C /w/c commit -m x"})
            )]),
        );
        let (head, tail) = next.split_at(next.len() / 2);
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut f, head).unwrap();
        assert!(activity.collect(&files).is_empty());
        std::io::Write::write_all(&mut f, tail).unwrap();
        let found = activity.collect(&files);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].target, PathBuf::from("/w/c"));
        // Only what grew was read: far less than the file.
        assert!(activity.read - after_first < text.len() as u64);
        // A transcript no longer recent is forgotten.
        activity.collect(&[]);
        assert!(activity.files.is_empty());
    }

    fn rollout(kind: &str, payload: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"timestamp": "t", "type": kind, "payload": payload}))
            .unwrap()
    }
    fn exec(script: &str) -> Vec<u8> {
        rollout(
            "response_item",
            serde_json::json!({"type": "custom_tool_call", "name": "exec", "status": "completed", "call_id": "c", "input": script}),
        )
    }

    #[test]
    fn codex_patches_and_commits_count_with_the_turns_folder_and_reads_do_not() {
        // SB-132: a Codex session's work, from its rollout.
        let mut cwd = None;
        assert!(
            codex_evidence(
                &exec("tools.exec_command({cmd: \"git -C /w/b commit -m x\"})"),
                &mut cwd
            )
            .is_empty(),
            "no folder yet"
        );
        codex_evidence(
            &rollout(
                "turn_context",
                serde_json::json!({"cwd": "/w/app", "model": "m"}),
            ),
            &mut cwd,
        );
        assert_eq!(cwd, Some(PathBuf::from("/w/app")));
        let targets = |line: Vec<u8>, cwd: &mut Option<PathBuf>| -> Vec<String> {
            codex_evidence(&line, cwd)
                .into_iter()
                .map(|w| {
                    assert_eq!(w.cwd, PathBuf::from("/w/app"));
                    w.target.to_string_lossy().into_owned()
                })
                .collect()
        };
        let script = "const r = await tools.exec_command({cmd: \"git add -A && git commit -m 'a; b'\", workdir: \"/w/lib\"});\ntext(await tools.exec_command({cmd: 'git status'}));";
        assert_eq!(targets(exec(script), &mut cwd), ["/w/lib"]);
        let patch = "await tools.apply_patch(`*** Begin Patch\n*** Update File: src/a.rs\n@@\n-x\n+y\n*** Add File: /w/docs/new.md\n+z\n*** End Patch`);";
        assert_eq!(targets(exec(patch), &mut cwd), ["/w/app/src", "/w/docs"]);
        let escaped = "await tools.apply_patch(\"*** Begin Patch\\n*** Delete File: ../other/x.txt\\n*** End Patch\");";
        assert_eq!(targets(exec(escaped), &mut cwd), ["/w/other"]);
        let dynamic = "await tools.exec_command({cmd: `git -C ${dir} commit -m x`});";
        assert!(
            targets(exec(dynamic), &mut cwd).is_empty(),
            "decided at run time: skipped"
        );
        let legacy = rollout(
            "response_item",
            serde_json::json!({"type": "function_call", "name": "shell", "call_id": "c",
                "arguments": serde_json::json!({"command": ["bash", "-lc", "git commit -m x"], "workdir": "/w/old"}).to_string()}),
        );
        assert_eq!(targets(legacy, &mut cwd), ["/w/old"]);
        let message = rollout(
            "response_item",
            serde_json::json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "run git commit in /w/x"}]}),
        );
        assert!(
            targets(message, &mut cwd).is_empty(),
            "text about a commit is not a commit"
        );
    }

    #[test]
    fn a_rollout_read_from_its_tail_takes_the_folder_its_session_started_in() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rollout-2026-10-19T00-00-00-x.jsonl");
        let mut text = rollout(
            "session_meta",
            serde_json::json!({"cwd": "/w/app", "id": "s"}),
        );
        text.push(b'\n');
        let filler = rollout(
            "event_msg",
            serde_json::json!({"type": "token_count", "info": "x".repeat(1000)}),
        );
        while (text.len() as u64) < TAIL_BYTES + 4096 {
            text.extend_from_slice(&filler);
            text.push(b'\n');
        }
        text.extend(exec(
            "await tools.exec_command({cmd: \"git commit -m x\"});",
        ));
        text.push(b'\n');
        fs::write(&path, &text).unwrap();
        let mut activity = Activity::default();
        let found = activity.collect(std::slice::from_ref(&path));
        assert_eq!(
            found,
            [Worked {
                cwd: PathBuf::from("/w/app"),
                target: PathBuf::from("/w/app"),
            }]
        );
    }

    #[test]
    fn collection_stops_between_transcripts_without_returning_partial_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.jsonl");
        let second = dir.path().join("second.jsonl");
        let call = |target: &str| {
            line(
                "/w/app",
                serde_json::json!([tool("Edit", serde_json::json!({"file_path": target}))]),
            )
        };
        fs::write(&first, call("/w/one/a.rs")).unwrap();
        fs::write(&second, call("/w/two/b.rs")).unwrap();
        let mut activity = Activity::default();
        let mut checks = 0;
        let found = activity.collect_while(&[first, second], || {
            checks += 1;
            checks > 1
        });
        assert!(found.is_none(), "a stop returns no partial evidence");
        assert_eq!(checks, 2, "the next transcript is not read after the stop");
        assert_eq!(activity.files.len(), 1);
    }
}
