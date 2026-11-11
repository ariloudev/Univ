// An open cluster's record: release 12's HIP 32754 record with its measurements replaced by one
// age, the values Hunt & Reffert 2023 publish for ASCC 5 in the recorded answer
// (crates/univ-ingest/tests/fixtures/vizier/hunt-reffert-2023.tsv): logAge50 7.32589102,
// logAge16 6.96985722 and logAge84 7.54225826, kept as Univ Ingest keeps them.

import type { Measurement, ObjectRecord } from "../api/objects.ts";
import { R12_HIP_32754 } from "./records/index.ts";

const [low, mid, high] = [6.969_857_22, 7.325_891_02, 7.542_258_26];
const years = 10 ** mid;

/** ASCC 5's age, as Univ Server gives it. */
export const ASCC_5_AGE: Measurement = {
  entry: 0,
  source: "hunt-reffert-2023",
  quantity: "age",
  n: 0,
  kind_of_value: "estimated",
  native: { value: mid, unit: "log_year", uncertainty: { form: "credible_interval", minus: mid - low, plus: high - mid, probability: 0.68 } },
  normalized: { value: years, unit: "year", uncertainty: { form: "credible_interval", minus: years - 10 ** low, plus: 10 ** high - years, probability: 0.68 } },
};

/** A record holding ASCC 5's age alone among its measurements. */
export const OPEN_CLUSTER: ObjectRecord = { ...R12_HIP_32754, measurements: [ASCC_5_AGE] };
