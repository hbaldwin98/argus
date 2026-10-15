// A form view block: fields the person fills in, sent back to the agent
// with the form's submit action.
import { actionButton } from './actions.js';
import { el } from './dom.js';
/** What the person has typed in a form not yet submitted, by the form's
 *  path and the field's name. A view is redrawn whole when anything in it
 *  changes, which would otherwise put the defaults back under their
 *  fingers. Text, a number's text, a choice or a date as typed; a toggle as
 *  its state. */
const drafts = new Map();
export function form(block, ctx) {
    const prefix = `${ctx.path}|`;
    const reads = [];
    const grid = el('div', { class: 'vform-grid' }, block.fields.map((field, n) => {
        const drawn = control(field, `${ctx.path}-${n}`, `${prefix}${field.name}`);
        reads.push(() => drawn.read());
        return el('div', { class: `vform-field${drawn.wide ? ' wide' : ''}${drawn.inline ? ' inline' : ''}` }, drawn.inline
            ? el('label', { class: 'vform-check', for: `${ctx.path}-${n}` }, drawn.node, field.label)
            : [
                el('label', { class: 'vform-label', for: `${ctx.path}-${n}` }, field.label),
                drawn.node,
            ]);
    }));
    const submit = actionButton(block.submit, ctx, () => {
        const answers = reads.map((read) => read()).filter((a) => a !== null);
        for (const key of drafts.keys())
            if (key.startsWith(prefix))
                drafts.delete(key);
        return { form: block.id, answers };
    });
    return el('form', { class: 'vform', onsubmit: (e) => e.preventDefault() }, grid, el('div', { class: 'vform-foot' }, submit));
}
function control(field, id, draft) {
    const input = field.input;
    const kept = drafts.get(draft);
    const text = (fallback) => (typeof kept === 'string' ? kept : fallback);
    const remember = (e) => {
        const t = e.target;
        drafts.set(draft, t.value);
    };
    switch (input.kind) {
        case 'text': {
            const common = {
                id,
                class: 'vform-input',
                placeholder: input.placeholder,
                oninput: remember,
            };
            const node = input.multiline
                ? el('textarea', { ...common, rows: 4 })
                : el('input', { ...common, type: 'text' });
            node.value = text(input.default ?? '');
            return {
                node,
                wide: input.multiline,
                inline: false,
                read: () => ({ name: field.name, value: { kind: 'text', value: node.value } }),
            };
        }
        case 'number': {
            const node = el('input', {
                id,
                class: 'vform-input',
                type: 'number',
                min: input.min,
                max: input.max,
                step: 'any',
                oninput: remember,
            });
            node.value = text(input.default == null ? '' : String(input.default));
            return {
                node,
                wide: false,
                inline: false,
                read: () => {
                    const n = node.value.trim() === '' ? NaN : Number(node.value);
                    return Number.isFinite(n)
                        ? { name: field.name, value: { kind: 'number', value: n } }
                        : null;
                },
            };
        }
        case 'choice': {
            const node = el('select', { id, class: 'vform-input', onchange: remember }, input.options.map((o) => el('option', { value: o }, o)));
            const wanted = text(input.default ?? input.options[0] ?? '');
            if (input.options.includes(wanted))
                node.value = wanted;
            return {
                node,
                wide: false,
                inline: false,
                read: () => ({ name: field.name, value: { kind: 'text', value: node.value } }),
            };
        }
        case 'toggle': {
            const node = el('input', {
                id,
                type: 'checkbox',
                onchange: (e) => drafts.set(draft, e.target.checked),
            });
            node.checked = typeof kept === 'boolean' ? kept : input.default;
            return {
                node,
                wide: false,
                inline: true,
                read: () => ({ name: field.name, value: { kind: 'bool', value: node.checked } }),
            };
        }
        case 'date': {
            const node = el('input', { id, class: 'vform-input', type: 'date', oninput: remember });
            node.value = text(input.default ?? '');
            return {
                node,
                wide: false,
                inline: false,
                read: () => ({ name: field.name, value: { kind: 'text', value: node.value } }),
            };
        }
    }
}
