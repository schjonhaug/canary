export const GITHUB_REPOSITORY_URL = "https://github.com/schjonhaug/canary"

export const installOptions = [
  {
    id: "umbrel",
    name: "Umbrel",
    logo: "/images/nodes/umbrel.svg",
    url: "https://apps.umbrel.com/app/canary",
  },
  {
    id: "start9",
    name: "Start9",
    logo: "/images/nodes/start9.svg",
    url: "https://marketplace.start9.com/?registry=https%3A%2F%2Fcommunity-registry.start9.com%2F&id=canary",
  },
  {
    id: "mynode",
    name: "myNode",
    logo: "/images/nodes/mynode.svg",
    url: "https://mynodebtc.com/",
  },
] as const

export const sourceOption = {
  id: "github",
  name: "GitHub",
  logo: "/images/github.svg",
  url: GITHUB_REPOSITORY_URL,
} as const

export type InstallOptionId = (typeof installOptions)[number]["id"]
