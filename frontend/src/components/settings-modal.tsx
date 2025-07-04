"use client"

import { useState, useEffect } from "react"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { CheckCircle, XCircle, Loader2, Eye, EyeOff } from "lucide-react"

interface TwilioConfig {
  account_sid: string
  auth_token: string
  messaging_service_sid: string
}

interface SettingsModalProps {
  isOpen: boolean
  onClose: () => void
}

export function SettingsModal({ isOpen, onClose }: SettingsModalProps) {
  const [config, setConfig] = useState<TwilioConfig | null>(null)
  const [formData, setFormData] = useState({
    account_sid: "",
    auth_token: "",
    messaging_service_sid: "",
  })
  const [isLoading, setIsLoading] = useState(false)
  const [isSaving, setIsSaving] = useState(false)
  const [validationResult, setValidationResult] = useState<{
    isValid: boolean
    message: string
  } | null>(null)
  const [showAuthToken, setShowAuthToken] = useState(false)

  useEffect(() => {
    if (isOpen) {
      fetchConfig()
    }
  }, [isOpen])

  const fetchConfig = async () => {
    setIsLoading(true)
    try {
      const response = await fetch("http://127.0.0.1:3000/twilio/config")
      if (response.ok) {
        const data = await response.json()
        setConfig(data)
        setFormData({
          account_sid: data.account_sid || "",
          auth_token: data.auth_token || "",
          messaging_service_sid: data.messaging_service_sid || "",
        })
      } else if (response.status === 404) {
        setConfig(null)
        setFormData({
          account_sid: "",
          auth_token: "",
          messaging_service_sid: "",
        })
      }
    } catch (error) {
      console.error("Failed to fetch Twilio config:", error)
    } finally {
      setIsLoading(false)
    }
  }

  const handleSave = async () => {
    setIsSaving(true)
    setValidationResult(null)
    
    try {
      // Include development mode flag to skip Twilio validation
      const requestBody = {
        ...formData,
        skip_validation: process.env.NODE_ENV === "development"
      }

      const response = await fetch("http://127.0.0.1:3000/twilio/config", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify(requestBody),
      })

      if (response.ok) {
        const result = await response.json()
        // Refresh the config from the backend
        await fetchConfig()
        setValidationResult({
          isValid: true,
          message: result.message || "Configuration saved successfully!",
        })
      } else {
        const error = await response.json()
        setValidationResult({
          isValid: false,
          message: error.error || "Failed to save configuration",
        })
      }
    } catch (error) {
      setValidationResult({
        isValid: false,
        message: `Error: ${error}`,
      })
    } finally {
      setIsSaving(false)
    }
  }

  const handleInputChange = (field: keyof TwilioConfig, value: string) => {
    setFormData(prev => ({
      ...prev,
      [field]: value,
    }))
    setValidationResult(null)
  }

  const isFormValid = formData.account_sid && formData.auth_token && formData.messaging_service_sid

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>
            Configure your Twilio SMS settings for transaction notifications
          </DialogDescription>
        </DialogHeader>

        {isLoading ? (
          <div className="flex items-center justify-center py-8">
            <Loader2 className="h-8 w-8 animate-spin" />
          </div>
        ) : (
          <div className="space-y-6">
            {/* Current Configuration Status */}
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  Current Configuration
                  {config ? (
                    <Badge variant="default" className="gap-1">
                      <CheckCircle size={12} />
                      Configured
                    </Badge>
                  ) : (
                    <Badge variant="destructive" className="gap-1">
                      <XCircle size={12} />
                      Not Configured
                    </Badge>
                  )}
                </CardTitle>
                <CardDescription>
                  {config 
                    ? "Twilio is configured and ready to send SMS notifications"
                    : "Configure Twilio to enable SMS notifications for wallet transactions"
                  }
                </CardDescription>
              </CardHeader>
              {config && (
                <CardContent>
                  <div className="space-y-2 text-sm">
                    <div>
                      <strong>Account SID:</strong> {config.account_sid}
                    </div>
                    <div>
                      <strong>Auth Token:</strong> {showAuthToken ? config.auth_token : "••••••••••••••••"}
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => setShowAuthToken(!showAuthToken)}
                        className="ml-2 h-6 px-2"
                      >
                        {showAuthToken ? <EyeOff size={12} /> : <Eye size={12} />}
                      </Button>
                    </div>
                    <div>
                      <strong>Messaging Service SID:</strong> {config.messaging_service_sid}
                    </div>
                  </div>
                </CardContent>
              )}
            </Card>

            {/* Configuration Form */}
            <Card>
              <CardHeader>
                <CardTitle>Twilio Configuration</CardTitle>
                <CardDescription>
                  Enter your Twilio credentials to enable SMS notifications
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4">
                <div className="space-y-2">
                  <Label htmlFor="account_sid">Account SID</Label>
                  <Input
                    id="account_sid"
                    type="text"
                    placeholder="ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
                    value={formData.account_sid}
                    onChange={(e) => handleInputChange("account_sid", e.target.value)}
                  />
                </div>

                <div className="space-y-2">
                  <Label htmlFor="auth_token">Auth Token</Label>
                  <div className="relative">
                    <Input
                      id="auth_token"
                      type={showAuthToken ? "text" : "password"}
                      placeholder="Your Twilio Auth Token"
                      value={formData.auth_token}
                      onChange={(e) => handleInputChange("auth_token", e.target.value)}
                    />
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      className="absolute right-2 top-1/2 -translate-y-1/2 h-6 px-2"
                      onClick={() => setShowAuthToken(!showAuthToken)}
                    >
                      {showAuthToken ? <EyeOff size={12} /> : <Eye size={12} />}
                    </Button>
                  </div>
                </div>

                <div className="space-y-2">
                  <Label htmlFor="messaging_service_sid">Messaging Service SID</Label>
                  <Input
                    id="messaging_service_sid"
                    type="text"
                    placeholder="MGxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
                    value={formData.messaging_service_sid}
                    onChange={(e) => handleInputChange("messaging_service_sid", e.target.value)}
                  />
                </div>

                {/* Validation Result */}
                {validationResult && (
                  <div className={`p-3 rounded-lg flex items-center gap-2 ${
                    validationResult.isValid 
                      ? "bg-green-50 text-green-800 border border-green-200" 
                      : "bg-red-50 text-red-800 border border-red-200"
                  }`}>
                    {validationResult.isValid ? (
                      <CheckCircle size={16} />
                    ) : (
                      <XCircle size={16} />
                    )}
                    <span className="text-sm">{validationResult.message}</span>
                  </div>
                )}

                {/* Action Button */}
                <div className="flex justify-end pt-4">
                  <Button
                    onClick={handleSave}
                    disabled={!isFormValid || isSaving}
                    className="gap-2"
                  >
                    {isSaving ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
                    Save Configuration
                  </Button>
                </div>
              </CardContent>
            </Card>
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}