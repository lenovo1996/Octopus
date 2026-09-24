import { describe, expect, it } from "vitest";
import { detectLanguage, highlightDiffLine, highlightLine } from "../../src/lib/diff/highlight";

describe("diff highlight", () => {
  it("detects languages by extension", () => {
    expect(detectLanguage("src/main.rs")).toBe("c");
    expect(detectLanguage("app.py")).toBe("hash");
    expect(detectLanguage("style.css")).toBe("c");
    expect(detectLanguage("page.html")).toBe("angle");
    expect(detectLanguage("Makefile")).toBeNull();
    expect(detectLanguage("notes.txt")).toBeNull();
  });

  it("escapes markup when no language matches", () => {
    expect(highlightLine("<script>alert(1)</script>", null)).toBe(
      "&lt;script&gt;alert(1)&lt;/script&gt;"
    );
  });

  it("never emits raw angle brackets", () => {
    const html = highlightLine(`if (a < b) { s = "<x>"; } // done <ok>`, "c");
    expect(html).not.toContain("<x>");
    expect(html).not.toContain("<ok>");
    expect(html).toContain("&lt;");
    expect(html).toContain("tok-kw");
    expect(html).toContain("tok-str");
    expect(html).toContain("tok-com");
  });

  it("wraps numbers and keywords", () => {
    const html = highlightLine("const total = count + 0x10;", "c");
    expect(html).toContain('<span class="tok-kw">const</span>');
    expect(html).toContain('<span class="tok-num">0x10</span>');
  });

  it("handles hash comments and unterminated strings", () => {
    const comment = highlightLine("name = value  # trailing", "hash");
    expect(comment).toContain('<span class="tok-com"># trailing</span>');
    const open = highlightLine(`path = "unterminated`, "hash");
    expect(open).toContain("tok-str");
    expect(open).toContain("unterminated");
  });

  it("handles block comments spanning the rest of the line", () => {
    const html = highlightLine("x = 1; /* note", "c");
    expect(html).toContain('<span class="tok-com">/* note</span>');
  });

  it("highlights by file path and escapes unknown extensions", () => {
    const html = highlightDiffLine("const total = 42; // sum", "src/app.ts");
    expect(html).toContain('<span class="tok-kw">const</span>');
    expect(html).toContain('<span class="tok-num">42</span>');
    expect(html).toContain("tok-com");
    expect(highlightDiffLine("<b>bold</b>", "notes.txt")).toBe("&lt;b&gt;bold&lt;/b&gt;");
  });

  it("detects php and jsx/tsx family by extension", () => {
    expect(detectLanguage("index.php")).toBe("php");
    expect(detectLanguage("App.jsx")).toBe("c");
    expect(detectLanguage("App.tsx")).toBe("c");
  });

  it("highlights php slash, hash and block comments plus strings", () => {
    expect(highlightDiffLine("$name = 'Ong'; // ten", "index.php")).toContain("tok-str");
    expect(highlightDiffLine("$x = 1; # ghi chu", "index.php")).toContain(
      '<span class="tok-com"># ghi chu</span>'
    );
    expect(highlightDiffLine("/* khoi */ $x = 2;", "index.php")).toContain("tok-com");
    expect(highlightDiffLine("if ($x > 1) { echo $x; }", "index.php")).toContain(
      '<span class="tok-kw">if</span>'
    );
  });
});
