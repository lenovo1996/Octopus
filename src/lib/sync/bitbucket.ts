export const BITBUCKET_TOKEN_URL = "https://id.atlassian.com/manage-profile/security/api-tokens";

/** Only Bitbucket Cloud HTTPS remotes can use the API-token integration. */
export function isBitbucketCloudHttps(remoteUrl: string | null): boolean {
  if (!remoteUrl) return false;
  try {
    const parsed = new URL(remoteUrl);
    return parsed.protocol === "https:" && parsed.hostname.toLowerCase() === "bitbucket.org";
  } catch {
    return false;
  }
}

export function bitbucketSubmitLabel(retryKind: string | null): string {
  return retryKind === "push" ? "Save & retry push" : "Save credential";
}
