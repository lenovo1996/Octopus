import type { RepoSnapshot } from "../ipc/types";

export const MAX_ALIAS_LENGTH = 80;

export function aliasValidationError(value: string): string | null {
  if (Array.from(value).some(char => {
    const code = char.codePointAt(0)!;
    return code < 32 || (code >= 127 && code <= 159) || code === 0x2028 || code === 0x2029;
  }) || Array.from(value.trim()).length > MAX_ALIAS_LENGTH) {
    return `Alias must be a single line of at most ${MAX_ALIAS_LENGTH} characters.`;
  }
  return null;
}

export function repositoryTabName(tab: { snapshot: RepoSnapshot; alias?: string | null }): string {
  return tab.alias ?? tab.snapshot.displayName;
}
