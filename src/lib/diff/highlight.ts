// Minimal offline syntax highlight for diff lines (no dependency).
// Detects a language from the file extension, then wraps comments,
// strings, numbers and keywords in spans. Everything is HTML-escaped
// first, so rendering the result with {@html} cannot inject markup.

const EXTENSIONS: Record<string, string> = {
  rs: "c",
  ts: "c",
  tsx: "c",
  mts: "c",
  cts: "c",
  js: "c",
  jsx: "c",
  mjs: "c",
  cjs: "c",
  php: "php",
  svelte: "c",
  go: "c",
  java: "c",
  c: "c",
  h: "c",
  cpp: "c",
  hpp: "c",
  cs: "c",
  css: "c",
  scss: "c",
  json: "c",
  kt: "c",
  swift: "c",
  scala: "c",
  py: "hash",
  rb: "hash",
  sh: "hash",
  bash: "hash",
  toml: "hash",
  yaml: "hash",
  yml: "hash",
  dockerfile: "hash",
  sql: "dash",
  lua: "dash",
  html: "angle",
  xml: "angle",
  vue: "angle",
  ini: "semi"
};

const BASE_KEYWORDS = [
  "if",
  "else",
  "for",
  "while",
  "return",
  "break",
  "continue",
  "function",
  "const",
  "let",
  "var",
  "new",
  "class",
  "extends",
  "import",
  "export",
  "from",
  "default",
  "switch",
  "case",
  "match",
  "struct",
  "enum",
  "impl",
  "trait",
  "fn",
  "pub",
  "use",
  "mod",
  "def",
  "lambda",
  "try",
  "catch",
  "throw",
  "async",
  "await",
  "true",
  "false",
  "null",
  "nil",
  "none",
  "self",
  "this",
  "super",
  "in",
  "of",
  "is",
  "as",
  "type",
  "interface",
  "static",
  "mut",
  "ref",
  "where",
  "do",
  "then",
  "end",
  "select",
  "where"
];

export function detectLanguage(path: string): string | null {
  const base = path.split("/").pop() ?? path;
  if (base === "Dockerfile") return "hash";
  const dot = base.lastIndexOf(".");
  if (dot < 0) return null;
  const group = EXTENSIONS[base.slice(dot + 1).toLowerCase()];
  return group ?? null;
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

interface Rule {
  group: string;
  lineComments?: string[];
  blockComment?: [string, string];
  angleComment?: boolean;
}

function ruleFor(group: string): Rule {
  if (group === "hash") return { group, lineComments: ["#"] };
  if (group === "dash") return { group, lineComments: ["--"] };
  if (group === "semi") return { group, lineComments: [";"] };
  if (group === "angle") return { group, angleComment: true };
  // PHP shares C-style strings/keywords but also allows `#` comments.
  if (group === "php") return { group, lineComments: ["//", "#"], blockComment: ["/*", "*/"] };
  return { group, lineComments: ["//"], blockComment: ["/*", "*/"] };
}

const KEYWORDS = new Set(BASE_KEYWORDS);
const WORD = /[A-Za-z_$][A-Za-z0-9_$]*/y;
const NUMBER = /(?:0x[0-9a-fA-F]+|\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)/y;

/** Highlight one raw line for a file path; output is escaped HTML safe for {@html}. */
export function highlightDiffLine(text: string, path: string): string {
  return highlightLine(text, detectLanguage(path));
}

/** Highlight one raw line; output is escaped HTML safe for {@html}. */
export function highlightLine(text: string, group: string | null): string {
  if (group === null) return escapeHtml(text);
  const rule = ruleFor(group);
  let out = "";
  let i = 0;
  const n = text.length;
  while (i < n) {
    const rest = text.slice(i);
    // Line comment to end of line.
    const commentPrefix = rule.lineComments?.find((prefix) => rest.startsWith(prefix));
    if (commentPrefix) {
      out += `<span class="tok-com">${escapeHtml(rest)}</span>`;
      break;
    }
    // Block comment.
    if (rule.blockComment && rest.startsWith(rule.blockComment[0])) {
      const end = text.indexOf(rule.blockComment[1], i + 2);
      const stop = end < 0 ? n : end + 2;
      out += `<span class="tok-com">${escapeHtml(text.slice(i, stop))}</span>`;
      i = stop;
      continue;
    }
    // HTML comment.
    if (rule.angleComment && rest.startsWith("<!--")) {
      const end = text.indexOf("-->", i + 4);
      const stop = end < 0 ? n : end + 3;
      out += `<span class="tok-com">${escapeHtml(text.slice(i, stop))}</span>`;
      i = stop;
      continue;
    }
    // String with backslash escapes.
    const quote = rest[0];
    if (quote === '"' || quote === "'" || quote === "`") {
      let j = i + 1;
      while (j < n) {
        if (text[j] === "\\") {
          j += 2;
          continue;
        }
        if (text[j] === quote) {
          j += 1;
          break;
        }
        j += 1;
      }
      out += `<span class="tok-str">${escapeHtml(text.slice(i, j))}</span>`;
      i = j;
      continue;
    }
    // Number.
    NUMBER.lastIndex = i;
    const num = NUMBER.exec(text);
    if (num) {
      out += `<span class="tok-num">${escapeHtml(num[0])}</span>`;
      i += num[0].length;
      continue;
    }
    // Word: keyword or plain.
    WORD.lastIndex = i;
    const word = WORD.exec(text);
    if (word) {
      const escaped = escapeHtml(word[0]);
      out += KEYWORDS.has(word[0]) ? `<span class="tok-kw">${escaped}</span>` : escaped;
      i += word[0].length;
      continue;
    }
    out += escapeHtml(text[i]);
    i += 1;
  }
  return out;
}
