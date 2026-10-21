// The parts of CodeMirror the editor uses, gathered into one module. `tsc`
// type-checks the page against these; `npm run build` then replaces what it
// wrote for this file with esbuild's bundle of them, so the page loads one
// script with one copy of each package and needs no bundler of its own.

export {
  ChangeSet,
  Compartment,
  RangeSet,
  RangeSetBuilder,
  EditorSelection,
  EditorState,
  StateEffect,
  StateField,
  Text,
  Transaction,
  Annotation,
  type ChangeSpec,
  type Extension,
  type Range,
} from '@codemirror/state';
export {
  Decoration,
  EditorView,
  GutterMarker,
  ViewPlugin,
  gutter,
  WidgetType,
  drawSelection,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
  type DecorationSet,
  type ViewUpdate,
} from '@codemirror/view';
export { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
export { StreamLanguage, bracketMatching, indentUnit, type Language } from '@codemirror/language';
export { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
export { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
export { highlightTree, tagHighlighter, tags, type Highlighter } from '@lezer/highlight';
export { TreeFragment, type Input, type PartialParse } from '@lezer/common';
export { rust } from '@codemirror/lang-rust';
export { javascript } from '@codemirror/lang-javascript';
export { python } from '@codemirror/lang-python';
export { css } from '@codemirror/lang-css';
export { json } from '@codemirror/lang-json';
export { markdown } from '@codemirror/lang-markdown';
export { yaml } from '@codemirror/lang-yaml';
export { toml } from '@codemirror/legacy-modes/mode/toml';
export { csharp } from '@codemirror/legacy-modes/mode/clike';
export { Vim, vim } from '@replit/codemirror-vim';
