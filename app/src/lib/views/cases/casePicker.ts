// The layout of the case "Add tracks" picker: the real Tracks table with its own remembered
// columns and sort, apart from the Tracks view and the Mix tab's table.
import { defaultPrefs, type ViewPrefs } from "../tracks/columns";

export const CASE_PICKER_KEY = "pm.cases.picker.v1";

/** The Tracks view's columns with Cases shown (the point of picking from a crate), and room for
 *  the waveform and the colour header. */
export function casePickerDefaults(): ViewPrefs {
  const d = defaultPrefs();
  d.visible.cases = true;
  d.widths.wave = 90;
  d.widths.colour = 74;
  // the picker is narrower than the Tracks view: drop the auto energy estimate and narrow Tags so
  // the Cases column is not cut off
  d.visible.autoenergy = false;
  d.widths.tags = 120;
  return d;
}
