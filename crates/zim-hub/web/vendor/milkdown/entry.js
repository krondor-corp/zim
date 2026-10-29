// Entry for the lazily-loaded Crepe bundle (see build.sh). The hub SPA
// imports the built `/static/milkdown/milkdown.js` on first .md open.
import { Crepe } from "@milkdown/crepe";
import "@milkdown/crepe/theme/common/style.css";
import "@milkdown/crepe/theme/classic.css";
import "katex/dist/katex.min.css";

// Mount a WYSIWYG markdown editor in `el`. `onChange(markdown)` fires on
// every edit after create(); the Rust side compares against the baseline
// (`getMarkdown()` right after mount) so merely opening a note, which
// Milkdown re-serializes (list markers, escaping), isn't an edit.
export async function mount(el, markdown, onChange) {
  const crepe = new Crepe({
    root: el,
    defaultValue: markdown,
    // No side `+` / drag handle (BlockEdit also owns the `/` menu).
    features: { [Crepe.Feature.BlockEdit]: false },
  });
  crepe.on((l) => l.markdownUpdated((_ctx, md) => onChange(md)));
  await crepe.create();
  return {
    getMarkdown: () => crepe.getMarkdown(),
    destroy: () => crepe.destroy(),
  };
}
