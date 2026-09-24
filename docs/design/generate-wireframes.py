#!/usr/bin/env python3
"""Regenerate the three design-only SVG wireframes; no app or Git operations."""
from pathlib import Path
from html import escape

OUT = Path(__file__).resolve().parent
C = dict(bg="#111618", panel="#192124", raised="#222d31", border="#35464b",
         text="#e5eeeb", muted="#a8bbb5", mint="#66d9b2", selected="#203d34",
         cyan="#66cde0", blue="#90b6ff", violet="#c39bf3", warn="#f1c66d")


def make(state):
    parts = ['<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="900" viewBox="0 0 1440 900" role="img" aria-labelledby="title desc">',
             f'<title id="title">GitDock — {escape(state)} wireframe</title>',
             '<desc id="desc">Proposed dark three-pane desktop Git GUI. Repository navigation on the left, commit graph in the center, contextual inspector on the right. Illustrative data, not a running application.</desc>']

    def rect(x, y, w, h, fill, radius=0, stroke=None):
        parts.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{radius}" fill="{fill}"' + (f' stroke="{stroke}"' if stroke else '') + '/>')

    def line(x1, y1, x2, y2, color=None, width=1):
        parts.append(f'<path d="M{x1} {y1}L{x2} {y2}" stroke="{color or C["border"]}" stroke-width="{width}" fill="none"/>')

    def text(x, y, value, cls='', color=None):
        font = 'DejaVu Sans Mono' if cls == 'mono' else 'DejaVu Sans'
        size = 12 if cls in ('mono', 'small') else 15 if cls == 'title' else 13
        parts.append(f'<text x="{x}" y="{y}" font-family="{font}" font-size="{size}" font-style="normal" font-weight="400" fill="{color or C["text"]}">{escape(value)}</text>')

    def button(x, y, w, label, primary=False, disabled=False):
        rect(x, y, w, 30, C['mint'] if primary else C['raised'], 4, None if primary else C['border'])
        text(x+10, y+20, label, 'small', '#10251d' if primary else C['muted'] if disabled else C['text'])

    def chip(x, y, label, color):
        w = len(label)*6.5 + 16
        rect(x, y-15, w, 22, C['raised'], 4, color)
        text(x+8, y, label, 'small', color)
        return w

    rect(0, 0, 1440, 900, C['bg'])
    rect(0, 0, 1440, 48, C['panel'])
    text(18, 30, 'GitDock', 'title', C['mint'])
    button(122, 9, 164, 'acme / desktop-app')
    button(298, 9, 128, 'main   /   HEAD')
    rect(930, 9, 270, 30, C['bg'], 4, C['border'])
    text(944, 29, 'Search history…    Ctrl+F', 'small', C['muted'])
    button(1214, 9, 102, 'Identity')
    button(1328, 9, 94, 'Menu')

    rect(0, 48, 1440, 44, C['panel'])
    line(0, 48, 1440, 48)
    for x, w, label in [(12, 62, 'Open'), (82, 65, 'Clone'), (155, 54, 'Init'),
                        (238, 66, 'Fetch'), (312, 60, 'Pull'), (380, 65, 'Push'),
                        (464, 82, 'Branch'), (554, 72, 'Stash'), (634, 104, 'Pop stash')]:
        button(x, 55, w, label)
    text(1042, 76, 'origin/main    ↑ 2    ↓ 0', 'small', C['muted'])
    text(1270, 76, 'Fetched 2m ago', 'small', C['muted'])

    rect(0, 92, 220, 784, C['panel'])
    rect(1060, 92, 380, 784, C['panel'])
    line(220, 92, 220, 876)
    line(1060, 92, 1060, 876)
    line(0, 92, 1440, 92)
    text(18, 120, 'WORKSPACE', 'small', C['muted'])
    rect(8, 134, 204, 34, C['selected'] if state == 'Working changes' else C['raised'], 4)
    text(20, 156, 'Working changes')
    text(185, 156, '3' if state != 'Merge conflict' else '2', color=C['mint'])
    sections = [(202, 'LOCAL', [('main', True), ('feature/graph', False), ('fix/sidebar', False)]),
                (354, 'REMOTE', [('origin/main', False), ('origin/feature/graph', False)]),
                (474, 'STASHES', [('WIP: graph spacing', False)]),
                (564, 'TAGS', [('v0.1.0', False)])]
    for y, heading, entries in sections:
        text(18, y, '⌄  ' + heading, 'small', C['muted'])
        for i, (label, selected) in enumerate(entries):
            yy = y+32*(i+1)
            if selected:
                rect(8, yy-21, 204, 30, C['selected'], 4)
            text(28, yy, label, color=C['mint'] if selected else C['text'])
            if selected:
                text(176, yy, '●', color=C['mint'])
    text(18, 674, 'PULL REQUESTS', 'small', C['muted'])
    text(28, 703, 'Planned after MVP', 'small', C['muted'])
    text(18, 756, 'SUBMODULES', 'small', C['muted'])
    text(28, 785, 'Read-only in MVP', 'small', C['muted'])

    rect(221, 93, 839, 39, C['panel'])
    text(238, 117, 'History', 'title')
    text(335, 117, 'All refs', color=C['muted'])
    text(870, 117, '200 commits loaded', 'small', C['muted'])
    rect(221, 132, 839, 32, C['raised'])
    text(239, 153, 'Graph', 'small', C['muted'])
    text(390, 153, 'Commit', 'small', C['muted'])
    text(838, 153, 'Author', 'small', C['muted'])
    text(966, 153, 'Time', 'small', C['muted'])

    subjects = ['Refine repository navigation', 'Merge feature/graph into main', 'Fix graph lane spacing',
                'Keep row selection while refreshing', 'Add graph keyboard navigation', 'Polish branch labels',
                'Add virtual commit list', 'Merge fix/sidebar into main', 'Handle empty repositories',
                'Add recent repositories', 'Restore panel widths on launch', 'Align toolbar actions',
                'Show active branch and upstream', 'Add commit details panel', 'Render unified text diffs',
                'Handle binary file previews', 'Parse porcelain status records', 'Add repository discovery',
                'Create shared design tokens', 'Build application shell']
    lanes = [0, 0, 1, 0, 1, 1, 1, 0, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    colors = [C['mint'], C['cyan'], C['violet']]
    # Each edge maps a child row to a parent row in illustrative topological order.
    edges = [(0,1),(1,3),(1,2),(2,4),(3,7),(4,5),(5,6),(6,10),(7,10),
             (7,8),(8,9),(9,12),(10,11),(11,12),(12,13),(13,14),(14,15),
             (15,16),(16,17),(17,18),(18,19)]
    selected = 1 if state == 'Commit details' else 0
    for i in range(len(subjects)):
        y=180+i*32
        if i == selected:
            rect(221, y-16, 839, 32, C['selected'])
        line(221, y+16, 1060, y+16, '#1c2629')
    for child, parent in edges:
        x1, x2 = 266+lanes[child]*26, 266+lanes[parent]*26
        y1, y2 = 180+child*32, 180+parent*32
        color = colors[max(lanes[child], lanes[parent])]
        parts.append(f'<path d="M{x1} {y1} C{x1} {(y1+y2)/2} {x2} {(y1+y2)/2} {x2} {y2}" fill="none" stroke="{color}" stroke-width="2"/>')
    for i, subject in enumerate(subjects):
        y=180+i*32
        parts.append(f'<circle cx="{266+lanes[i]*26}" cy="{y}" r="4" fill="{C["bg"]}" stroke="{colors[lanes[i]]}" stroke-width="2"/>')
        text(348, y+4, f'{(0xa3f2190-i*983):07x}', 'mono', C['muted'])
        text(420, y+4, subject)
        text(838, y+4, ['Linh', 'Minh', 'Alex'][i % 3], 'small', C['muted'])
        text(966, y+4, f'{i+1}h ago', 'small', C['muted'])
    chip(757, 184, 'main', C['mint'])
    chip(706, 376, 'feature/graph', C['cyan'])
    chip(722, 472, 'fix/sidebar', C['violet'])
    chip(420, 851, 'main', C['mint'])
    chip(488, 851, 'feature/graph', C['cyan'])
    chip(609, 851, 'fix/sidebar', C['violet'])
    text(770, 851, 'Scroll to load more history', 'small', C['muted'])

    x=1078
    text(x, 120, state, 'title')
    line(1060, 132, 1440, 132)
    if state == 'Working changes':
        text(x, 160, 'main  ·  3 changed files', color=C['muted'])
        text(x, 205, 'UNSTAGED  2', 'small', C['muted'])
        button(1310, 183, 110, 'Stage all')
        for yy, name, tag in [(244, 'src/lib/graph.ts', 'M'), (280, 'src/app/App.svelte', 'M')]:
            rect(x, yy-14, 12, 12, C['bg'], 2, C['border'])
            text(x+25, yy, tag, color=C['warn'])
            text(x+47, yy, name, 'mono')
        text(x, 335, 'STAGED  1', 'small', C['muted'])
        button(1310, 313, 110, 'Unstage all')
        rect(1068, 350, 364, 34, C['selected'], 4)
        text(x, 372, 'A', color=C['mint'])
        text(x+25, 372, 'src/lib/tokens.css', 'mono')
        text(x, 418, 'tokens.css', 'mono')
        text(1320, 418, '+3  −0', 'small', C['mint'])
        rect(x, 434, 344, 134, C['bg'], 4)
        text(x+10, 459, '@@ -0,0 +1,3 @@', 'mono', C['muted'])
        for yy, value in [(488, '+ :root {'), (515, '+   --accent: #66d9b2;'), (542, '+ }')]:
            rect(x+3, yy-18, 338, 26, '#173b2d')
            text(x+10, yy, value, 'mono', '#b4f2cf')
        text(x, 622, 'COMMIT MESSAGE', 'small', C['muted'])
        rect(x, 638, 344, 36, C['bg'], 4, C['border'])
        text(x+10, 661, 'Add shared design tokens')
        rect(x, 686, 344, 91, C['bg'], 4, C['border'])
        text(x+10, 709, 'Description (optional)', 'small', C['muted'])
        button(x, 798, 344, 'Commit 1 file', True)
        text(x, 850, 'Ctrl+Enter from message editor', 'small', C['muted'])
    elif state == 'Commit details':
        text(x, 166, 'Merge feature/graph into main', 'title')
        text(x, 199, 'a3f1db9   ·   Copy OID', 'mono', C['mint'])
        text(x, 235, 'Author     Minh <minh@example.test>', 'small')
        text(x, 263, 'Committed  22 Sep 2026, 10:32 +07:00', 'small', C['muted'])
        chip(x, 300, '2 parents', C['cyan'])
        text(x, 345, 'Bring graph navigation into main.')
        text(x, 402, 'COMPARE TO PARENT', 'small', C['muted'])
        button(x, 418, 344, 'Parent 1  /  a3f160b')
        text(x, 490, 'CHANGED FILES  2', 'small', C['muted'])
        rect(1068, 508, 364, 34, C['selected'], 4)
        text(x, 531, 'M   src/lib/graph.ts', 'mono')
        text(x, 569, 'M   src/app/App.svelte', 'mono')
        rect(x, 599, 344, 158, C['bg'], 4)
        text(x+10, 624, '@@ -12,3 +12,3 @@', 'mono', C['muted'])
        rect(x+3, 640, 338, 28, '#43272b')
        text(x+10, 659, '- const laneWidth = 16;', 'mono', '#ffc0c6')
        rect(x+3, 670, 338, 28, '#173b2d')
        text(x+10, 689, '+ const laneWidth = 20;', 'mono', '#b4f2cf')
        text(x+10, 729, '  renderGraph(commits);', 'mono')
        button(x, 798, 344, 'Back to working changes')
    else:
        rect(x, 149, 344, 66, '#3d3421', 4)
        text(x+12, 174, 'Merge in progress', color=C['warn'])
        text(x+12, 199, 'feature/graph → main', 'small', C['warn'])
        text(x, 253, 'UNRESOLVED  2', 'small', C['muted'])
        rect(1068, 270, 364, 34, C['selected'], 4)
        text(x, 292, 'U   src/lib/graph.ts', 'mono', C['warn'])
        text(x, 329, 'U   src/app/App.svelte', 'mono', C['warn'])
        text(x, 382, 'CURRENT  /  main', 'small', C['muted'])
        rect(x, 398, 344, 50, C['bg'], 4)
        text(x+10, 428, 'const laneWidth = 16;', 'mono')
        text(x, 482, 'INCOMING  /  feature/graph', 'small', C['muted'])
        rect(x, 498, 344, 50, C['bg'], 4)
        text(x+10, 528, 'const laneWidth = 20;', 'mono')
        button(x, 570, 166, 'Use current')
        button(x+178, 570, 166, 'Use incoming')
        text(x, 632, 'Or edit externally, then refresh.', 'small', C['muted'])
        button(x, 652, 344, 'Refresh working result')
        button(x, 697, 344, 'Mark selected file resolved')
        line(1060, 758, 1440, 758)
        text(x, 785, 'Resolve all files to complete merge.', 'small', C['warn'])
        button(x, 803, 166, 'Abort merge')
        button(x+178, 803, 166, 'Complete merge', disabled=True)

    rect(0, 876, 1440, 24, C['raised'])
    text(12, 893, 'main   ·   2 unresolved conflicts' if state == 'Merge conflict' else 'main   ·   Ready', 'small', C['warn'] if state == 'Merge conflict' else C['mint'])
    text(510, 893, 'DESIGN WIREFRAME  /  ILLUSTRATIVE DATA', 'small', C['muted'])
    text(1260, 893, 'Linux MVP   ·   Help', 'small', C['muted'])
    parts.append('</svg>')
    return '\n'.join(parts)+'\n'


if __name__ == '__main__':
    for filename, state in [('01-working-changes.svg', 'Working changes'),
                            ('02-commit-details.svg', 'Commit details'),
                            ('03-merge-conflict.svg', 'Merge conflict')]:
        (OUT / filename).write_text(make(state), encoding='utf-8')
        print(filename)
