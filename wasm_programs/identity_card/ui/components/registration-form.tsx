"use client"

import type React from "react"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { onvmClient, type RegisterRequest } from "@/lib/onvm-client"

const THEMES = [
  { name: "blue", color: "#60a5fa" },
  { name: "purple", color: "#a78bfa" },
  { name: "green", color: "#4ade80" },
  { name: "red", color: "#f87171" },
  { name: "orange", color: "#fb923c" },
  { name: "pink", color: "#f472b6" },
  { name: "dark", color: "#64748b" },
  { name: "light", color: "#3b82f6" },
]

export function RegistrationForm() {
  const [formData, setFormData] = useState({
    name: "",
    email: "",
    bio: "",
    avatar_url: "",
    theme: "blue",
  })
  const [loading, setLoading] = useState(false)
  const [result, setResult] = useState<any>(null)
  const [error, setError] = useState<string | null>(null)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setLoading(true)
    setResult(null)
    setError(null)

    try {
      const request: RegisterRequest = {
        op: "register",
        name: formData.name,
        email: formData.email,
        bio: formData.bio || undefined,
        avatar_url: formData.avatar_url || undefined,
        theme: formData.theme || undefined,
      }

      const response = await onvmClient.registerIdentity(request)
      setResult(response)
    } catch (err: any) {
      console.error("Registration error:", err)
      setError(err.message || "Failed to register identity")
    } finally {
      setLoading(false)
    }
  }

  return (
    <section id="register" className="py-24 px-4 relative">
      <div className="absolute inset-0 bg-gradient-to-b from-transparent via-primary/5 to-transparent pointer-events-none" />

      <div className="max-w-4xl mx-auto relative z-10">
        <div className="text-center mb-12 space-y-4">
          <h2 className="text-4xl md:text-5xl font-bold">Create Your Identity</h2>
          <p className="text-xl text-muted-foreground text-balance">
            Generate a verifiable digital identity card with cryptographic proof
          </p>
        </div>

        <Card className="border-border/50 backdrop-blur-sm bg-card/50">
          <CardHeader>
            <CardTitle>Identity Registration</CardTitle>
            <CardDescription>Fill in your details to create a new identity card</CardDescription>
          </CardHeader>
          <CardContent>
            <form onSubmit={handleSubmit} className="space-y-6">
              <div className="grid md:grid-cols-2 gap-6">
                <div className="space-y-2">
                  <Label htmlFor="name">Full Name *</Label>
                  <Input
                    id="name"
                    value={formData.name}
                    onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                    placeholder="John Doe"
                    required
                    className="bg-background/50"
                  />
                </div>

                <div className="space-y-2">
                  <Label htmlFor="email">Email Address *</Label>
                  <Input
                    id="email"
                    type="email"
                    value={formData.email}
                    onChange={(e) => setFormData({ ...formData, email: e.target.value })}
                    placeholder="john@example.com"
                    required
                    className="bg-background/50"
                  />
                </div>
              </div>

              <div className="space-y-2">
                <Label htmlFor="bio">Bio</Label>
                <Textarea
                  id="bio"
                  value={formData.bio}
                  onChange={(e) => setFormData({ ...formData, bio: e.target.value })}
                  placeholder="Tell us about yourself..."
                  rows={3}
                  className="bg-background/50 resize-none"
                />
              </div>

              <div className="space-y-2">
                <Label htmlFor="avatar">Avatar URL</Label>
                <Input
                  id="avatar"
                  type="url"
                  value={formData.avatar_url}
                  onChange={(e) => setFormData({ ...formData, avatar_url: e.target.value })}
                  placeholder="https://example.com/avatar.jpg"
                  className="bg-background/50"
                />
              </div>

              <div className="space-y-3">
                <Label>Theme Color</Label>
                <div className="grid grid-cols-4 md:grid-cols-8 gap-3">
                  {THEMES.map((theme) => (
                    <button
                      key={theme.name}
                      type="button"
                      onClick={() => setFormData({ ...formData, theme: theme.name })}
                      className={`group relative aspect-square rounded-lg transition-all hover:scale-110 ${
                        formData.theme === theme.name
                          ? "ring-2 ring-primary ring-offset-2 ring-offset-background scale-110"
                          : ""
                      }`}
                      style={{ backgroundColor: theme.color }}
                    >
                      {formData.theme === theme.name && (
                        <svg
                          className="absolute inset-0 m-auto w-6 h-6 text-white drop-shadow-lg"
                          fill="currentColor"
                          viewBox="0 0 20 20"
                        >
                          <path
                            fillRule="evenodd"
                            d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z"
                            clipRule="evenodd"
                          />
                        </svg>
                      )}
                      <span className="sr-only">{theme.name}</span>
                    </button>
                  ))}
                </div>
              </div>

              <Button
                type="submit"
                disabled={loading}
                className="w-full bg-primary hover:bg-primary/90 text-primary-foreground py-6 text-lg font-semibold"
              >
                {loading ? (
                  <>
                    <svg className="animate-spin -ml-1 mr-3 h-5 w-5" fill="none" viewBox="0 0 24 24">
                      <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                      <path
                        className="opacity-75"
                        fill="currentColor"
                        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                      />
                    </svg>
                    Creating Identity...
                  </>
                ) : (
                  "Create Identity Card"
                )}
              </Button>
            </form>

            {error && (
              <div className="mt-6 p-4 rounded-lg bg-destructive/10 border border-destructive/20">
                <p className="text-sm font-semibold text-destructive mb-2">✗ Registration Failed</p>
                <p className="text-sm text-destructive">{error}</p>
              </div>
            )}

            {result && (
              <div className="mt-6 p-4 rounded-lg bg-primary/10 border border-primary/20">
                <p className="text-sm font-semibold text-primary mb-2">✓ Identity Created Successfully!</p>
                <p className="text-sm text-muted-foreground font-mono">ID: {result.id}</p>
                <div className="mt-4">
                  <h3 className="text-sm font-medium mb-2">Your Identity Card:</h3>
                  <div 
                    className="border rounded-lg p-2 bg-white"
                    dangerouslySetInnerHTML={{ __html: result.card_svg }} 
                  />
                </div>
              </div>
            )}
          </CardContent>
        </Card>
      </div>
    </section>
  )
}