/**
 * Backend errors that carry a name (an agent's), so they cannot be listed verbatim in
 * src/adapter.ts. Each `template` is the Rust `format!` string with `{}` written as `{name}`;
 * the name is bounded and plain so a match is safe to show. The window shows the sentence and
 * translates the template (L10N-04). `scripts/check-error-vocabulary.mjs` fails when a backend
 * `format!` error has no template here; `scripts/check-locale.mjs` collects the templates.
 */
export const FORMATTED_ERRORS: readonly string[] = [
  '{name} is not installed. Install it first, then launch it again.',
  '{name} cannot take over a workflow: an agent in a chain loads MCP servers and takes a prompt without a person (switchboard agents list).',
];

const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const PATTERNS = FORMATTED_ERRORS.map((template) => ({
  template,
  pattern: new RegExp(`^${escape(template).replace(escape('{name}'), '([A-Za-z0-9][A-Za-z0-9 ._()+-]{0,39})')}$`),
}));

/** The template a backend sentence was formatted from, and the name in it; null for any other text. */
export function matchFormatted(text: string): { template: string; name: string } | null {
  for (const { template, pattern } of PATTERNS) {
    const match = pattern.exec(text);
    if (match) return { template, name: match[1] };
  }
  return null;
}
