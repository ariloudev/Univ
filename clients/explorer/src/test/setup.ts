import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

import { forgetRecords } from "../api/objects.ts";
import { forgetSources } from "../api/sources.ts";

afterEach(() => {
  cleanup();
  // Each test serves its own answers: none is read from another's.
  forgetRecords();
  forgetSources();
});
