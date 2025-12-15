"use client"

import { useState, useEffect } from "react"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { onvmClient } from "@/lib/onvm-client"

export function StatsSection() {
  const [stats, setStats] = useState({
    total_identities: 0,
    total_storage_bytes: 0,
    merkle_root: "",
  })
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const loadStats = async () => {
    setLoading(true)
    setError(null)
    
    try {
      const response = await onvmClient.getStats()
      setStats({
        total_identities: response.total_identities,
        total_storage_bytes: response.total_storage_bytes,
        merkle_root: response.merkle_root,
      })
    } catch (err: any) {
      console.error("Failed to load stats:", err)
      setError(err.message || "Failed to load statistics")
    } finally {
      setLoading(false)
    }
  }

  // Load stats on component mount
  useEffect(() => {
    loadStats()
  }, [])

  if (loading) {
    return (
      <section className="py-24 px-4 relative overflow-hidden">
        <div className="max-w-6xl mx-auto">
          <div className="text-center mb-16">
            <h2 className="text-4xl md:text-5xl font-bold">Loading Statistics...</h2>
          </div>
          <div className="grid md:grid-cols-3 gap-6">
            {[1, 2, 3].map((i) => (
              <Card key={i} className="border-border/50 bg-card/50 backdrop-blur-sm h-48 animate-pulse" />
            ))}
          </div>
        </div>
      </section>
    )
  }

  if (error) {
    return (
      <section className="py-24 px-4 relative overflow-hidden">
        <div className="max-w-6xl mx-auto">
          <div className="text-center mb-16">
            <h2 className="text-4xl md:text-5xl font-bold">System Statistics</h2>
          </div>
          <div className="p-6 rounded-lg bg-destructive/10 border border-destructive/20 text-center">
            <p className="text-destructive">{error}</p>
            <button 
              onClick={loadStats}
              className="mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90"
            >
              Retry
            </button>
          </div>
        </div>
      </section>
    )
  }

  return (
    <section className="py-24 px-4 relative overflow-hidden">
      {/* Background decoration */}
      <div className="absolute inset-0 bg-gradient-to-t from-primary/10 via-transparent to-transparent pointer-events-none" />

      <div className="max-w-6xl mx-auto relative z-10">
        <div className="text-center mb-16 space-y-4">
          <h2 className="text-4xl md:text-5xl font-bold">System Statistics</h2>
          <p className="text-xl text-muted-foreground text-balance">Real-time metrics from the identity blockchain</p>
        </div>

        <div className="grid md:grid-cols-3 gap-6">
          {/* Total Identities */}
          <Card className="border-border/50 bg-card/50 backdrop-blur-sm relative overflow-hidden group hover:shadow-xl hover:shadow-primary/10 transition-all">
            <div className="absolute inset-0 bg-gradient-to-br from-primary/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity" />
            <CardHeader className="relative z-10">
              <div className="flex items-center justify-between">
                <CardTitle className="text-sm font-medium text-muted-foreground uppercase tracking-wide">
                  Total Identities
                </CardTitle>
                <svg
                  className="w-8 h-8 text-primary/30"
                  fill="none"
                  strokeWidth="2"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z"
                  />
                </svg>
              </div>
            </CardHeader>
            <CardContent className="relative z-10">
              <div className="text-4xl md:text-5xl font-bold bg-gradient-to-r from-primary to-chart-2 bg-clip-text text-transparent">
                {stats.total_identities.toLocaleString()}
              </div>
              <p className="text-sm text-muted-foreground mt-2">Verified on-chain</p>
            </CardContent>
          </Card>

          {/* Storage Used */}
          <Card className="border-border/50 bg-card/50 backdrop-blur-sm relative overflow-hidden group hover:shadow-xl hover:shadow-chart-3/10 transition-all">
            <div className="absolute inset-0 bg-gradient-to-br from-chart-3/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity" />
            <CardHeader className="relative z-10">
              <div className="flex items-center justify-between">
                <CardTitle className="text-sm font-medium text-muted-foreground uppercase tracking-wide">
                  Storage Used
                </CardTitle>
                <svg
                  className="w-8 h-8 text-chart-3/30"
                  fill="none"
                  strokeWidth="2"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M5 19a2 2 0 01-2-2V7a2 2 0 012-2h4l2 2h4a2 2 0 012 2v1M5 19h14a2 2 0 002-2v-5a2 2 0 00-2-2H9a2 2 0 00-2 2v5a2 2 0 01-2 2z"
                  />
                </svg>
              </div>
            </CardHeader>
            <CardContent className="relative z-10">
              <div className="text-4xl md:text-5xl font-bold bg-gradient-to-r from-chart-3 to-chart-4 bg-clip-text text-transparent">
                {(stats.total_storage_bytes / 1024 / 1024).toFixed(2)} MB
              </div>
              <p className="text-sm text-muted-foreground mt-2">Across all identities</p>
            </CardContent>
          </Card>

          {/* Merkle Root */}
          <Card className="border-border/50 bg-card/50 backdrop-blur-sm relative overflow-hidden group hover:shadow-xl hover:shadow-chart-5/10 transition-all">
            <div className="absolute inset-0 bg-gradient-to-br from-chart-5/5 to-transparent opacity-0 group-hover:opacity-100 transition-opacity" />
            <CardHeader className="relative z-10">
              <div className="flex items-center justify-between">
                <CardTitle className="text-sm font-medium text-muted-foreground uppercase tracking-wide">
                  Merkle Root
                </CardTitle>
                <svg
                  className="w-8 h-8 text-chart-5/30"
                  fill="none"
                  strokeWidth="2"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    d="M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.749c0 5.592 3.824 10.29 9 11.623 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.571-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285z"
                  />
                </svg>
              </div>
            </CardHeader>
            <CardContent className="relative z-10">
              <div className="text-2xl md:text-3xl font-bold font-mono bg-gradient-to-r from-chart-5 to-primary bg-clip-text text-transparent break-all">
                {stats.merkle_root.substring(0, 12)}...
              </div>
              <p className="text-sm text-muted-foreground mt-2">Cryptographic verification</p>
            </CardContent>
          </Card>
        </div>

        {/* Additional Info */}
        <div className="mt-12 p-8 rounded-2xl border border-border/50 bg-card/30 backdrop-blur-sm">
          <div className="flex flex-col md:flex-row items-center justify-between gap-6">
            <div className="space-y-2">
              <h3 className="text-2xl font-bold">Ready to create your identity?</h3>
              <p className="text-muted-foreground">Join thousands of users with verified digital identity cards</p>
            </div>
            <a
              href="#register"
              className="inline-flex items-center justify-center px-8 py-4 rounded-xl bg-primary text-primary-foreground font-semibold hover:bg-primary/90 transition-all hover:scale-105 shadow-lg shadow-primary/25 whitespace-nowrap"
            >
              Get Started
              <svg className="ml-2 w-5 h-5" fill="none" strokeWidth="2" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" d="M13 7l5 5m0 0l-5 5m5-5H6" />
              </svg>
            </a>
          </div>
        </div>
      </div>
    </section>
  )
}