"use client"

import { Suspense } from "react"
import { FeedContent } from "@/components/feed/feed-content"

export function DiscoverContent() {
  return (
    <Suspense fallback={<div className="p-6 text-muted-foreground">Loading discover...</div>}>
      <FeedContent initialTab="discover" fixedDiscover hideToggle title="Discover" />
    </Suspense>
  )
}
