"use client"

import * as React from "react"
import { useIsMobile } from "@/hooks/useIsMobile"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  Drawer,
  DrawerClose,
  DrawerContent,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
} from "@/components/ui/drawer"

interface ResponsiveModalProps {
  children: React.ReactNode
  open?: boolean
  onOpenChange?: (open: boolean) => void
}

function ResponsiveModal({ children, ...props }: ResponsiveModalProps) {
  const isMobile = useIsMobile()

  if (isMobile) {
    return <Drawer {...props}>{children}</Drawer>
  }
  return <Dialog {...props}>{children}</Dialog>
}

function ResponsiveModalContent({
  className,
  children,
  showCloseButton,
  onOpenAutoFocus,
  ...props
}: React.ComponentProps<typeof DialogContent>) {
  const isMobile = useIsMobile()

  if (isMobile) {
    return (
      <DrawerContent className={className} {...props}>
        <div className="overflow-y-auto px-4 pb-4">
          {children}
        </div>
      </DrawerContent>
    )
  }
  return (
    <DialogContent
      className={className}
      showCloseButton={showCloseButton}
      onOpenAutoFocus={onOpenAutoFocus}
      {...props}
    >
      {children}
    </DialogContent>
  )
}

function ResponsiveModalHeader({
  className,
  ...props
}: React.ComponentProps<"div">) {
  const isMobile = useIsMobile()

  if (isMobile) {
    return <DrawerHeader className={className} {...props} />
  }
  return <DialogHeader className={className} {...props} />
}

function ResponsiveModalTitle({
  className,
  ...props
}: React.ComponentProps<typeof DialogTitle>) {
  const isMobile = useIsMobile()

  if (isMobile) {
    return <DrawerTitle className={className} {...props} />
  }
  return <DialogTitle className={className} {...props} />
}

function ResponsiveModalDescription({
  className,
  ...props
}: React.ComponentProps<typeof DialogDescription>) {
  const isMobile = useIsMobile()

  if (isMobile) {
    return <DrawerDescription className={className} {...props} />
  }
  return <DialogDescription className={className} {...props} />
}

function ResponsiveModalFooter({
  className,
  ...props
}: React.ComponentProps<"div">) {
  const isMobile = useIsMobile()

  if (isMobile) {
    return <DrawerFooter className={className} {...props} />
  }
  return <DialogFooter className={className} {...props} />
}

function ResponsiveModalClose({
  ...props
}: React.ComponentProps<typeof DialogClose>) {
  const isMobile = useIsMobile()

  if (isMobile) {
    return <DrawerClose {...props} />
  }
  return <DialogClose {...props} />
}

export {
  ResponsiveModal,
  ResponsiveModalClose,
  ResponsiveModalContent,
  ResponsiveModalDescription,
  ResponsiveModalFooter,
  ResponsiveModalHeader,
  ResponsiveModalTitle,
}
