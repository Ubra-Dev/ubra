import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  CONTACT_URL,
  ISSUES_URL,
  LICENSE_URL,
  PRIVACY_URL,
  RELEASES_URL,
  REPO_URL,
  SECURITY_EMAIL,
  SITE_URL,
  SUPPORT_EMAIL,
  TERMS_URL,
} from "../src/lib/site.ts";

describe("site links", () => {
  it("points the website and legal pages at getubra.com", () => {
    assert.equal(SITE_URL, "https://getubra.com");
    assert.equal(PRIVACY_URL, "https://getubra.com/privacy");
    assert.equal(TERMS_URL, "https://getubra.com/terms");
    assert.equal(CONTACT_URL, "https://getubra.com/contact");
  });

  it("exposes the public support and security addresses", () => {
    assert.equal(SUPPORT_EMAIL, "support@getubra.com");
    assert.equal(SECURITY_EMAIL, "security@getubra.com");
  });

  it("keeps working repository sections on GitHub", () => {
    assert.equal(REPO_URL, "https://github.com/Ubra-Dev/ubra");
    assert.equal(ISSUES_URL, `${REPO_URL}/issues`);
    assert.equal(RELEASES_URL, `${REPO_URL}/releases`);
    assert.equal(LICENSE_URL, `${REPO_URL}/blob/main/LICENSE`);
  });
});
