"use client"

import Image from "next/image"
import { useBlockHeader } from "@/hooks/useBlockHeader"
import { useRelativeTime } from "@/hooks/useRelativeTime"

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
            <p className="text-muted-foreground text-sm">Bitcoin Wallet Alert System</p>
          </div>
        </div>
        
        {/* Blockchain Info */}
        {blockHeader && (
          <div className="flex items-center gap-4 text-sm">
            <div className="flex items-center gap-2">
              <span className="text-muted-foreground">Block height:</span>
              <span className="font-mono font-medium">{blockHeader.height.toLocaleString()}</span>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-muted-foreground">Time:</span>
              <span>{blockHeaderTime}</span>
            </div>
          </div>
        )}
        
        <a 
          href="https://github.com/schjonhaug/canary" 
          target="_blank" 
          rel="noopener noreferrer"
          className="text-muted-foreground hover:text-accent transition-colors flex items-center gap-2"
        >
          <Image
            src="/images/github.svg"
            alt="GitHub"
            width={20}
            height={20}
            className="h-5 w-5"
          />
          <span>GitHub</span>
        </a>
      </div>
    </footer>
  )
}