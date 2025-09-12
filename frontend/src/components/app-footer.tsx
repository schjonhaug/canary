"use client"

import Image from "next/image"
import { useBlockHeader } from "@/hooks/useBlockHeader"
import { useRelativeTime } from "@/hooks/useRelativeTime"
import { BuildInfo } from "./build-info"

export function AppFooter() {
  const { blockHeader } = useBlockHeader()
  const blockHeaderTime = useRelativeTime(blockHeader?.timestamp)

  return (
    <footer className="mt-16 pt-8 border-t border-border">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-4">
          <Image
            src="/images/canary-in-a-coalmine.svg"
            alt="Canary Logo"
            width={48}
            height={48}
            className="h-12 w-12"
          />
          <div>
            <h3 className="text-lg font-bold tracking-wide">Canary</h3>
            {blockHeader ? (
              <p className="text-muted-foreground text-sm">
                Block {blockHeader.height.toLocaleString()} • {blockHeaderTime}
              </p>
            ) : (
              <p className="text-muted-foreground text-sm">Connecting to network...</p>
            )}
          </div>
        </div>
        
        <BuildInfo />
      </div>
    </footer>
  )
}