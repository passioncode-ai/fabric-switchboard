# Creating an organization in one step: its GitHub repositories and folders (SB-264, SB-265)

Design note, 2026-11-05. Status: designed. Owner: fabric-switchboard.

## What the operator asked

In the operator's words (session 38adf103, 2026-11-05): adding an organization is inconvenient.
Folders are chosen one at a time and several cannot be picked at once. When the organization has a
GitHub organization, its repositories should come in by themselves, chosen from the GitHub
organizations the person can see; some of them are not on this computer yet and should be cloned.
A folder the person already has should be attachable to the organization while it is created.

## What is true today (evidence)

- *+ New organization* asks for a name and domains only (`organizationDialog`, `src/main.ts`);
  folders belong to groups, chosen in the group dialog (`projectDialog`, `foldersPicker`).
- Every folder picker picks one folder (`pick_folder`, `src-tauri/src/main.rs`, the dialog plugin's
  `pick_folder`); the backend already takes many (`SaveProject.folders`, `ProjectEdit.add_folders`).
- No window path creates an organization and its group together; the organization card's
  *+ Group for {organization}* comes after (SCN-076).
- Switchboard does not talk to GitHub. A group's `sources` (`github.com/<owner>`, SB-117) make a
  repository already on disk whose `origin` has that owner join the group by itself
  (`attribution.rs` AT-10, the 30 s suggest pass); nothing clones.
- On the operator's Mac the GitHub CLI `gh` is installed and signed in and lists the person's login
  and eleven organizations (`gh api user/orgs`, 2026-11-05).

## Decisions

Recorded by fabric-switchboard under the operator's rule (decide as the operator would, safest
reversible option).

1. **Several folders at once (SB-264).** Every *Add folder…* in the window opens the system picker
   with several selection (`pick_folders`); each chosen folder is a row. One-folder pickers
   (*Change…*, a single destination) stay single.
2. **GitHub through the person's own `gh` (SB-265).** Switchboard runs `gh` (the GitHub CLI) and
   stores no GitHub token: who can see what is GitHub's and the person's sign-in there. Without
   `gh`, or signed out, the wizard has no GitHub step and says how to turn it on
   (`gh auth login`); the rest works.
3. **Owners.** The person's own login and every organization `gh api user/orgs` lists.
4. **Repositories.** `gh repo list <owner>` (at most 1000): name, URL, archived, fork. Each is
   matched with this computer by its `origin` (`host/owner/repo`, the repository map under the
   default roots and every group folder): *On this computer* (its folder) or *Not here yet*.
   Preselected: on disk; missing ones that are neither archived nor forks (*Clone*).
5. **Where clones go.** The folder holding most of that owner's clones already, else `~/DATA` when
   it exists, else the home folder; *Change…* picks another. A folder of the repository's name that
   already exists there is never written into: that repository is skipped and named.
6. **One save.** *Create* adds the organization, then its group: the organization's name, bound to
   it, every folder kept (on disk, cloned later, added by hand), `sources` `github.com/<owner>`, no
   accounts (the organization card's seat step follows, SCN-076). New repositories of that owner
   cloned later join by the source, as today.
7. **Cloning.** In the background, one at a time, with `gh repo clone <owner>/<name> <folder>`; each
   one cloned joins the group at once. The window shows *Cloning n of m*, then what was cloned and
   what was not, with why (exists, refused, failed). Closing the window does not stop it; quitting
   Switchboard stops it, and a half-cloned folder is removed.

## Not built here

Importing into an organization that already exists (its group's *Edit…* keeps *Add folders…*),
GitLab and Bitbucket, private repositories `gh` cannot see, and choosing accounts in the wizard
(the organization card does that).
