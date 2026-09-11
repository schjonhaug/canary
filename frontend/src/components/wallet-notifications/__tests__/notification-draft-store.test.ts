import {
  clearCreateNotificationDraft,
  clearEditNotificationDraft,
  getNotificationSession,
  resetNotificationDraftSessions,
  setCreateNotificationDraft,
  setEditNotificationDraft,
} from "../notification-draft-store"
import { DEFAULT_NEW_CONTACT_SETTINGS } from "../utils"
import { DEFAULT_NOTIFICATION_CONTENT_FIELDS } from "@/components/notification-content-fields-control"

const draft = {
  name: "Desk",
  methods: [{
    provider_type: "ntfy" as const,
    notification_target: "canary-0123456789abcdef0123456789abcdef",
    is_enabled: true,
    content_fields: { ...DEFAULT_NOTIFICATION_CONTENT_FIELDS },
  }],
  ...DEFAULT_NEW_CONTACT_SETTINGS,
}

describe("notification-draft-store", () => {
  beforeEach(() => {
    resetNotificationDraftSessions()
  })

  it("isolates drafts by wallet and destination", () => {
    setCreateNotificationDraft("wallet-a", {
      step: "alerts",
      draft,
      balanceDrafts: [],
      ntfyTopicWasEdited: false,
    })
    setEditNotificationDraft("wallet-b", "contact-1", { draft, balanceDrafts: [] })

    expect(getNotificationSession("wallet-a").create?.draft.name).toBe("Desk")
    expect(getNotificationSession("wallet-a").edits).toEqual({})
    expect(getNotificationSession("wallet-b").create).toBeUndefined()
    expect(getNotificationSession("wallet-b").edits["contact-1"]?.draft.name).toBe("Desk")
  })

  it("discards create and edit drafts independently", () => {
    setCreateNotificationDraft("wallet-a", {
      step: "privacy",
      draft,
      balanceDrafts: [],
      ntfyTopicWasEdited: true,
    })
    setEditNotificationDraft("wallet-a", "contact-1", { draft, balanceDrafts: [] })
    clearCreateNotificationDraft("wallet-a")

    expect(getNotificationSession("wallet-a").create).toBeUndefined()
    expect(getNotificationSession("wallet-a").edits["contact-1"]).toBeDefined()

    clearEditNotificationDraft("wallet-a", "contact-1")
    expect(getNotificationSession("wallet-a").edits["contact-1"]).toBeUndefined()
    expect(getNotificationSession("wallet-a").activeFlow).toBeNull()
  })
})
