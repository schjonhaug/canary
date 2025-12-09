import { compat, types as T } from "../deps.ts";

export const migration: T.ExpectedExports.migration =
  compat.migrations.fromMapping(
    {
      // Initial version - no migrations needed yet
      "0.1.0": {
        up: compat.migrations.updateConfig(
          (config: any) => {
            // Initial config structure
            return {
              network: config?.network || "mainnet",
              "electrum-source": config?.["electrum-source"] || "local",
              "external-electrum-url":
                config?.["external-electrum-url"] ||
                "ssl://electrum.blockstream.info:50002",
              "admin-notification-topic":
                config?.["admin-notification-topic"] || null,
            };
          },
          true,
          { version: "0.1.0", type: "up" }
        ),
        down: () => {
          throw new Error("Cannot downgrade to before initial version");
        },
      },
    },
    "0.1.0"
  );
