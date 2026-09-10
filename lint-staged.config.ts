import type { Configuration } from "lint-staged";

const config: Configuration = {
  "src/**/*.rs": () => "cargo fmt",
  "*.{js,jsx,ts,tsx,mjs,cjs}": ["bun run format --", "bun run lint --"],
};

export default config;
