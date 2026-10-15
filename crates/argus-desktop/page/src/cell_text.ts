// What a cell reads as in plain text, for what has to match it (a filter)
// or leave the page as text (a table copied as TSV).

import type { Value } from './bindings/Value';

export function cellText(v: Value): string {
  switch (v.kind) {
    case 'text':
    case 'code':
      return v.text;
    case 'number':
      return v.unit ? `${v.number} ${v.unit}` : String(v.number);
    case 'bool':
      return v.value ? 'yes' : 'no';
    case 'date':
      return v.date;
    case 'date_time':
      return v.at;
    case 'person':
      return v.name;
    case 'people':
      return v.names.join(', ');
    case 'chip':
      return v.chip.label;
    case 'chips':
      return v.chips.map((c) => c.label).join(', ');
    case 'link':
      return v.label;
    case 'ticket':
      return v.title ? `${v.key} ${v.title}` : v.key;
    case 'progress':
      return `${v.value}/${v.max}`;
    case 'duration':
      return `${v.ms} ms`;
    case 'path':
      return v.line === null ? v.path : `${v.path}:${v.line}`;
    case 'commit':
      return v.title ? `${v.sha} ${v.title}` : v.sha;
    case 'feature':
      return `#${v.number}`;
    case 'pane':
      return v.label ?? `pane ${v.pane}`;
    case 'empty':
      return '';
  }
}
