// Real records of release 12. An object's record is trimmed: each source's entries after its
// twelfth, and a group's members after its twelfth, are left out, with every measurement of what
// is kept. A structure's record is whole but for its members' columns the panel does not read.

import { readRecord } from "../../api/objects.ts";
import { readStructure } from "../../api/structures.ts";
import draco from "./draco.json";
import erassCluster from "./erass-cluster.json";
import greatAttractor from "./st-great-attractor.json";
import hip32754 from "./hip-32754.json";
import hugeLqg from "./st-huge-lqg.json";
import kbcVoid from "./st-kbc-void.json";
import laniakea from "./st-laniakea-supercluster.json";
import localVoid from "./st-local-void.json";
import m31 from "./m31.json";
import ngcGalaxy from "./ngc-galaxy.json";
import pgcGroup from "./pgc-group.json";
import sdssGalaxy from "./sdss-galaxy.json";
import sloanGreatWall from "./st-sloan-great-wall.json";
import vela from "./st-vela-supercluster.json";

export const R12_M31 = readRecord(m31);
export const R12_DRACO = readRecord(draco);
export const R12_GROUP = readRecord(pgcGroup);
export const R12_ERASS = readRecord(erassCluster);
export const R12_SDSS = readRecord(sdssGalaxy);
export const R12_NGC = readRecord(ngcGalaxy);
export const R12_HIP_32754 = readRecord(hip32754);


export const R12_GREAT_ATTRACTOR = readStructure(greatAttractor);
export const R12_HUGE_LQG = readStructure(hugeLqg);
export const R12_KBC_VOID = readStructure(kbcVoid);
export const R12_LANIAKEA = readStructure(laniakea);
export const R12_LOCAL_VOID = readStructure(localVoid);
export const R12_SLOAN_GREAT_WALL = readStructure(sloanGreatWall);
export const R12_VELA = readStructure(vela);
