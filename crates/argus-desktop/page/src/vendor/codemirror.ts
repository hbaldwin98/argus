// The parts of CodeMirror the editor uses, and tree-sitter's runtime,
// gathered into one module. `tsc`
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
export { bracketMatching, indentUnit } from '@codemirror/language';
export { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
export { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
export { Edit, Language, Parser, Query, type Point, type Tree as TSTree } from 'web-tree-sitter';
export { Vim, vim } from '@replit/codemirror-vim';
