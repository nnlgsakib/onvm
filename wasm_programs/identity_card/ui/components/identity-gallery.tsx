"use client"

import { useState, useEffect } from "react"
import { Card, CardContent } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { onvmClient } from "@/lib/onvm-client"
import type { IdentitySummary } from "@/lib/onvm-client"

export function IdentityGallery() {
  const [identities, setIdentities] = useState<IdentitySummary[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [selectedIdentity, setSelectedIdentity] = useState<IdentitySummary | null>(null)
  const [identityCardSvg, setIdentityCardSvg] = useState<string | null>(null)
  const [cardLoading, setCardLoading] = useState(false)

  const loadIdentities = async () => {
    setLoading(true)
    setError(null)
    
    try {
      const response = await onvmClient.listAllIdentities()
      setIdentities(response.identities)
    } catch (err: any) {
      console.error("Failed to load identities:", err)
      setError(err.message || "Failed to load identities")
    } finally {
      setLoading(false)
    }
  }

  const loadIdentityCard = async (identity: IdentitySummary) => {
    setCardLoading(true)
    setSelectedIdentity(identity)
    setIdentityCardSvg(null)
    
    try {
      const response = await onvmClient.getIdentityCard({
        op: "getcard",
        id: identity.id,
        format: "svg"
      })
      setIdentityCardSvg(response.card_svg)
    } catch (err: any) {
      console.error("Failed to load identity card:", err)
      setIdentityCardSvg(null)
    } finally {
      setCardLoading(false)
    }
  }

  // Load identities on component mount
  useEffect(() => {
    loadIdentities()
  }, [])

  return (
    <section id="gallery" className="py-24 px-4 relative">
      <div className="max-w-7xl mx-auto">
        <div className="text-center mb-16 space-y-4">
          <h2 className="text-4xl md:text-5xl font-bold">Identity Gallery</h2>
          <p className="text-xl text-muted-foreground text-balance max-w-2xl mx-auto">
            Browse verified identities with cryptographic proof
          </p>
        </div>

        {error && (
          <div className="mb-8 p-4 rounded-lg bg-destructive/10 border border-destructive/20 text-center">
            <p className="text-destructive">{error}</p>
          </div>
        )}

        <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-6">
          {identities.map((identity, index) => (
            <Card
              key={identity.id}
              className="group hover:shadow-xl hover:shadow-primary/10 transition-all duration-300 hover:-translate-y-1 border-border/50 bg-card/50 backdrop-blur-sm overflow-hidden"
              style={{ animationDelay: `${index * 100}ms` }}
            >
              <CardContent className="p-6 space-y-4">
                {/* Identity Card Preview */}
                <div className="aspect-[4/2.5] rounded-lg bg-gradient-to-br from-primary/20 to-chart-2/20 border border-border/50 p-4 flex items-center justify-center">
                  <div className="text-center space-y-2">
                    <div className="w-16 h-16 rounded-full bg-primary/30 mx-auto flex items-center justify-center text-2xl font-bold">
                      {identity.name.charAt(0)}
                    </div>
                    <div className="font-semibold text-lg">{identity.name}</div>
                    <div className="text-sm text-muted-foreground">{identity.email}</div>
                  </div>
                </div>

                {/* Identity Info */}
                <div className="space-y-2">
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-muted-foreground">ID:</span>
                    <code className="font-mono text-xs bg-secondary/50 px-2 py-1 rounded">
                      {identity.id.substring(0, 8)}...
                    </code>
                  </div>
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-muted-foreground">Theme:</span>
                    <span className="capitalize font-medium">{identity.theme}</span>
                  </div>
                  <div className="flex items-center justify-between text-sm">
                    <span className="text-muted-foreground">Created:</span>
                    <span>{new Date(identity.created_at * 1000).toLocaleDateString()}</span>
                  </div>
                </div>

                {/* Action Button */}
                <Button
                  variant="outline"
                  onClick={() => loadIdentityCard(identity)}
                  className="w-full group-hover:bg-primary group-hover:text-primary-foreground group-hover:border-primary transition-colors bg-transparent"
                >
                  View Full Card
                </Button>
              </CardContent>
            </Card>
          ))}
        </div>

        <div className="text-center mt-12">
          <Button
            onClick={loadIdentities}
            disabled={loading}
            size="lg"
            variant="outline"
            className="px-8 bg-transparent"
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
                Loading...
              </>
            ) : (
              "Load More Identities"
            )}
          </Button>
        </div>
      </div>

      {/* Identity Card Modal */}
      <Dialog open={!!selectedIdentity} onOpenChange={(open) => !open && setSelectedIdentity(null)}>
        <DialogContent className="max-w-2xl max-h-[90vh] overflow-y-auto">
          <DialogHeader>
            <DialogTitle>
              {selectedIdentity ? `${selectedIdentity.name}'s Identity Card` : "Identity Card"}
            </DialogTitle>
          </DialogHeader>
          
          {cardLoading ? (
            <div className="flex items-center justify-center h-64">
              <svg className="animate-spin h-12 w-12 text-primary" fill="none" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                <path
                  className="opacity-75"
                  fill="currentColor"
                  d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                />
              </svg>
            </div>
          ) : identityCardSvg ? (
            <div 
              className="border rounded-lg p-4 bg-white"
              dangerouslySetInnerHTML={{ __html: identityCardSvg }} 
            />
          ) : selectedIdentity ? (
            <div className="text-center py-8 text-muted-foreground">
              Failed to load identity card
            </div>
          ) : null}
        </DialogContent>
      </Dialog>
    </section>
  )
}