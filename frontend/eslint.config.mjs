import nextCoreWebVitals from "eslint-config-next/core-web-vitals";
import nextTypescript from "eslint-config-next/typescript";

const eslintConfig = [
  ...nextCoreWebVitals,
  ...nextTypescript,
  {
    ignores: [
      "jest.config.js",
      "jest.setup.js",
      "scripts/**/*.js",
      "next.config.ts",
    ],
  },
  {
    rules: {
      // Disable strict rule that wasn't enforced by next lint
      "react-hooks/set-state-in-effect": "off",
      "react-hooks/incompatible-library": "off",
    },
  },
];

export default eslintConfig;
