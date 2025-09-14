export function BuildInfo() {
  let buildInfo: { commit: string } | null = null
  
  try {
    // Try to import build-info.json - will throw if file doesn't exist
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    buildInfo = require("@/lib/build-info.json")
  } catch {
    // File doesn't exist (Umbrel deployment) - don't show version
    return null
  }
  
  if (!buildInfo) return null

  return (
    <div className="text-muted-foreground text-sm font-mono">
      {buildInfo.commit}
    </div>
  )
}