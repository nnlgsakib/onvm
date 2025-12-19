"use client"

import { useState } from "react"
import { Card } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { User, Bell, Lock } from "lucide-react"

export function SettingsContent() {
  const [emailNotifications, setEmailNotifications] = useState(true)
  const [pushNotifications, setPushNotifications] = useState(true)
  const [privateAccount, setPrivateAccount] = useState(false)

  const settingsSections = [
    {
      icon: User,
      title: "Account",
      description: "Manage your account settings",
      settings: [
        { label: "Username", value: "john_doe", type: "input" },
        { label: "Email", value: "john@example.com", type: "input" },
        { label: "Bio", value: "Full-stack developer", type: "textarea" },
      ],
    },
    {
      icon: Bell,
      title: "Notifications",
      description: "Control how you receive notifications",
      settings: [
        { label: "Email Notifications", value: emailNotifications, type: "switch", onChange: setEmailNotifications },
        { label: "Push Notifications", value: pushNotifications, type: "switch", onChange: setPushNotifications },
      ],
    },
    {
      icon: Lock,
      title: "Privacy",
      description: "Manage your privacy settings",
      settings: [{ label: "Private Account", value: privateAccount, type: "switch", onChange: setPrivateAccount }],
    },
  ]

  return (
    <div className="max-w-2xl mx-auto">
      {/* Header */}
      <div className="sticky top-0 z-40 bg-background/80 backdrop-blur-xl border-b border-border/50 p-4">
        <h1 className="text-2xl font-bold">Settings</h1>
        <p className="text-sm text-muted-foreground mt-1">Manage your account preferences</p>
      </div>

      {/* Settings Sections */}
      <div className="p-4 space-y-6">
        {settingsSections.map((section, index) => {
          const Icon = section.icon
          return (
            <Card key={index} className="p-6 bg-card/50 border-border/50 backdrop-blur-sm">
              <div className="flex items-center gap-3 mb-4">
                <div className="flex items-center justify-center w-10 h-10 rounded-xl bg-gradient-to-br from-primary/20 to-accent/20">
                  <Icon className="w-5 h-5 text-primary" />
                </div>
                <div>
                  <h2 className="text-lg font-semibold">{section.title}</h2>
                  <p className="text-sm text-muted-foreground">{section.description}</p>
                </div>
              </div>

              <div className="space-y-4">
                {section.settings.map((setting, settingIndex) => (
                  <div key={settingIndex} className="space-y-2">
                    <Label>{setting.label}</Label>
                    {setting.type === "input" && (
                      <Input
                        defaultValue={setting.value as string}
                        className="bg-background/50 border-border/50 focus:border-primary transition-colors"
                      />
                    )}
                    {setting.type === "switch" && (
                      <div className="flex items-center justify-between">
                        <span className="text-sm text-muted-foreground">{setting.value ? "Enabled" : "Disabled"}</span>
                        <Switch
                          checked={setting.value as boolean}
                          onCheckedChange={setting.onChange as (checked: boolean) => void}
                        />
                      </div>
                    )}
                  </div>
                ))}
              </div>
            </Card>
          )
        })}

        {/* Save Button */}
        <Button className="w-full bg-gradient-to-r from-primary to-accent hover:opacity-90 transition-all hover:scale-[1.02] transform shadow-lg shadow-primary/25">
          Save Changes
        </Button>

        {/* Danger Zone */}
        <Card className="p-6 bg-destructive/5 border-destructive/20">
          <h3 className="text-lg font-semibold text-destructive mb-2">Danger Zone</h3>
          <p className="text-sm text-muted-foreground mb-4">These actions are irreversible. Please be careful.</p>
          <div className="space-y-2">
            <Button
              variant="outline"
              className="w-full border-destructive/50 text-destructive hover:bg-destructive/10 bg-transparent"
            >
              Deactivate Account
            </Button>
            <Button variant="destructive" className="w-full">
              Delete Account
            </Button>
          </div>
        </Card>
      </div>
    </div>
  )
}
