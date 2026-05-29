/** Full, editable template shapes — mirror the Rust `OwnedTemplate` /
 * `OwnedSection` (see `src-tauri/src/models/template.rs`). The lightweight
 * `TemplateInfo` in `features/meetings/types.ts` is what the pickers use. */

export interface OwnedSection {
  id: string;
  heading: string;
  description: string;
  example?: string | null;
}

export interface OwnedTemplate {
  id: string;
  name: string;
  description: string;
  sections: OwnedSection[];
  builtin: boolean;
  template_example?: string | null;
}
