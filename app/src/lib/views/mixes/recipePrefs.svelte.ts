// The auto-transition settings (default recipe, length, hard-cut position, what to do about tools
// in the way). A per-person preference of the app, so it lives in the browser storage like the
// Tracks columns do (try/catch-guarded: the app works the same when storage is unavailable).
import { DEFAULT_RECIPE_PREFS, mergeRecipePrefs, type RecipeId, type RecipePrefs } from "$lib/state/transitionRecipes";

const STORAGE_KEY = "pm.mixes.recipes.v1";

class RecipeSettings {
  value = $state<RecipePrefs>({ ...DEFAULT_RECIPE_PREFS });
  /** The recipe and length each transition built this session came from, so its panel shows them
   * (the document itself only stores the curve and the tools). Not saved. */
  readonly built = new Map<string, { recipe: RecipeId; bars: number }>();

  constructor() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (raw) this.value = mergeRecipePrefs(JSON.parse(raw));
    } catch {
      // unavailable or corrupt: keep the defaults
    }
  }

  set(patch: Partial<RecipePrefs>) {
    this.value = mergeRecipePrefs({ ...this.value, ...patch });
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.value));
    } catch {
      // not remembered this time
    }
  }

  reset() {
    this.set({ ...DEFAULT_RECIPE_PREFS });
  }
}

export const recipePrefs = new RecipeSettings();
