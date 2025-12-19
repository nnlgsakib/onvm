"use client"

import { FeedContent } from "@/components/feed/feed-content"

export function TrendingContent() {
  return <FeedContent initialTab="discover" fixedDiscover hideToggle title="Trending" />
}
