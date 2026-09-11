import type { BalanceDraft, ContactDraft, WizardStep } from "./types"

export type ActiveNotificationFlow = { type: "create" } | { type: "edit"; contactId: string } | null

export type CreateNotificationDraft = {
  step: WizardStep
  draft: ContactDraft
  balanceDrafts: BalanceDraft[]
  ntfyTopicWasEdited: boolean
}

export type EditNotificationDraft = {
  draft: ContactDraft
  balanceDrafts: BalanceDraft[]
}

export type WalletNotificationSession = {
  activeFlow: ActiveNotificationFlow
  create?: CreateNotificationDraft
  edits: Record<string, EditNotificationDraft>
}

const sessions = new Map<string, WalletNotificationSession>()

function emptySession(): WalletNotificationSession {
  return { activeFlow: null, edits: {} }
}

function sessionFor(checksum: string): WalletNotificationSession {
  return sessions.get(checksum) ?? emptySession()
}

export function getNotificationSession(checksum: string): WalletNotificationSession {
  return sessionFor(checksum)
}

export function setActiveNotificationFlow(checksum: string, activeFlow: ActiveNotificationFlow) {
  const current = sessionFor(checksum)
  sessions.set(checksum, { ...current, activeFlow, edits: { ...current.edits } })
}

export function setCreateNotificationDraft(checksum: string, create: CreateNotificationDraft) {
  const current = sessionFor(checksum)
  sessions.set(checksum, {
    ...current,
    activeFlow: { type: "create" },
    create,
    edits: { ...current.edits },
  })
}

export function clearCreateNotificationDraft(checksum: string) {
  const current = sessionFor(checksum)
  sessions.set(checksum, {
    activeFlow: current.activeFlow?.type === "create" ? null : current.activeFlow,
    edits: { ...current.edits },
  })
}

export function setEditNotificationDraft(
  checksum: string,
  contactId: string,
  edit: EditNotificationDraft,
) {
  const current = sessionFor(checksum)
  sessions.set(checksum, {
    ...current,
    activeFlow: { type: "edit", contactId },
    edits: { ...current.edits, [contactId]: edit },
  })
}

export function clearEditNotificationDraft(checksum: string, contactId: string) {
  const current = sessionFor(checksum)
  const edits = { ...current.edits }
  delete edits[contactId]
  sessions.set(checksum, {
    activeFlow:
      current.activeFlow?.type === "edit" && current.activeFlow.contactId === contactId
        ? null
        : current.activeFlow,
    create: current.create,
    edits,
  })
}

export function resetNotificationDraftSessions() {
  sessions.clear()
}
