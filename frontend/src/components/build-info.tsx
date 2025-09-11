export function BuildInfo() {
  try {
    // Try to import build-info.json - will throw if file doesn't exist
    const buildInfo = require("@/lib/build-info.json")
    
    // Format: "v0.8.1 • 7ab035e" or just "7ab035e" if no tag
    const displayVersion = buildInfo.tag 
      ? `${buildInfo.tag} • ${buildInfo.commit}`
      : buildInfo.commit

    return (
      <div className="text-muted-foreground text-sm font-mono">
        {displayVersion}
      </div>
    )
  } catch {
    // File doesn't exist (Umbrel deployment) - don't show version
    return null
  }
}