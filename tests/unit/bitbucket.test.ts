import { describe, expect, it } from "vitest";
import { bitbucketSubmitLabel, isBitbucketCloudHttps } from "../../src/lib/sync/bitbucket";

describe("Bitbucket integration helpers", () => {
  it("accepts only Bitbucket Cloud HTTPS remotes", () => {
    expect(isBitbucketCloudHttps("https://bitbucket.org/workspace/repository.git")).toBe(true);
    expect(isBitbucketCloudHttps("https://user@bitbucket.org/workspace/repository.git")).toBe(true);
    expect(isBitbucketCloudHttps("git@bitbucket.org:workspace/repository.git")).toBe(false);
    expect(isBitbucketCloudHttps("https://bitbucket.org.evil.test/workspace/repository.git")).toBe(false);
  });

  it("makes the push retry explicit", () => {
    expect(bitbucketSubmitLabel("push")).toBe("Save & retry push");
    expect(bitbucketSubmitLabel("fetch")).toBe("Save credential");
  });
});
