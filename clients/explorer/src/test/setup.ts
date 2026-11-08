import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

import { forgetCaches } from "../api/client.ts";

afterEach(() => {
  cleanup();
  // Each test serves its own answers: none is read from another's.
  forgetCaches();
});
