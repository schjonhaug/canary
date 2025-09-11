export function BuildInfo() {
  let buildInfo: { tag?: string; commit: string } | null = null
  
  try {
    // Try to import build-info.json - will throw if file doesn't exist
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    buildInfo = require("@/lib/build-info.json")
  } catch {
    // File doesn't exist (Umbrel deployment) - don't show version
    return null
  }
  
  if (!buildInfo) return null
  
  // Format: "v0.8.1 • 7ab035e" or just "7ab035e" if no tag
  const displayVersion = buildInfo.tag 
    ? `${buildInfo.tag} • ${buildInfo.commit}`
    : buildInfo.commit

  return (
    <div className="text-muted-foreground text-sm font-mono">
      {displayVersion}
    </div>
  )
}