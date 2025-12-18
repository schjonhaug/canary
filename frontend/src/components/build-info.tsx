export function BuildInfo() {
  const commit = process.env.NEXT_PUBLIC_BUILD_COMMIT

  // Don't show if no commit info (e.g., Umbrel deployment without git)
  if (!commit) return null

  return (
    <div className="text-muted-foreground text-sm font-mono">
      {commit}
    </div>
  )
}
