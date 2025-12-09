import { compat, types as T } from "../deps.ts";

export const setConfig: T.ExpectedExports.setConfig = async (
  effects,
  input
) => {
  // deno-lint-ignore no-explicit-any
  const newConfig = input as any;

  // If using local Electrs, add it as a dependency
  const depsElectrs: T.DependsOn =
    newConfig?.["electrum-source"] === "local" ? { electrs: [] } : {};

  return await compat.setConfig(effects, input, {
    ...depsElectrs,
  });
};
