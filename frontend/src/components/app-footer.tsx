"use client"

import Image from "next/image"
import { Github, Heart } from "lucide-react"
import { useBlockHeader } from "@/hooks/useBlockHeader"
import { useRelativeTime } from "@/hooks/useRelativeTime"
import { useFormatters } from "@/hooks/useFormatters"
import { useAuth } from "@/contexts/auth-context"
import { BuildInfo } from "./build-info"
import Link from "next/link"
import { useTranslations } from "next-intl"

export function AppFooter() {
  const { blockHeader } = useBlockHeader()
  const blockHeaderTime = useRelativeTime(blockHeader?.timestamp)
  const { isCloudMode } = useAuth()
  const t = useTranslations('footer')
  const tCommon = useTranslations('common')
  const { formatNumber } = useFormatters()

  return (
    <footer className="mt-16 pt-8 border-t border-border">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-4">
          <Image
            src="/images/canary-in-a-coalmine.svg"
            alt={t('logoAlt')}
            width={48}
            height={48}
            className="h-12 w-12"
          />
          <div>
            <h3 className="text-lg font-bold tracking-wide">{t('appName')}</h3>
            {blockHeader ? (
              <p className="text-muted-foreground text-sm">
                {t('blockInfo', { height: formatNumber(blockHeader.height) })}
                {blockHeaderTime && ` • ${blockHeaderTime}`}
              </p>
            ) : (
              <p className="text-muted-foreground text-sm">{tCommon('connectingToNetwork')}</p>
            )}
          </div>
        </div>

        <div className="flex items-center gap-4">
          {!isCloudMode && (
            <Link
              href="/donations"
              className="flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors"
            >
              <Heart className="h-4 w-4" />
              {t('donations')}
            </Link>
          )}
          {!isCloudMode && (
            <a
              href="https://github.com/schjonhaug/canary"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors"
            >
              <Github className="h-4 w-4" />
              GitHub
            </a>
          )}
          {isCloudMode && <BuildInfo />}
        </div>
      </div>
    </footer>
  )
}
