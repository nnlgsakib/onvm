"use client"

import { Suspense } from "react"
import { FeedContent } from "@/components/feed/feed-content"

export function TrendingContent() {
  return (
    <Suspense fallback={<div className="p-6 text-muted-foreground">Loading trending...</div>}>
      <FeedContent initialTab="discover" fixedDiscover hideToggle title="Trending" />
    </Suspense>
  )
}
