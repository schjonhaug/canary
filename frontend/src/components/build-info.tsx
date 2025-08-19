import buildInfo from "@/lib/build-info.json"

export function BuildInfo() {
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