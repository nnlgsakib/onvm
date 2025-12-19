import { FeedLayout } from "@/components/feed/feed-layout"
import { FeedContent } from "@/components/feed/feed-content"
import { Suspense } from "react"

export default function FeedPage() {
  return (
    <FeedLayout>
      <Suspense fallback={<div className="p-6 text-muted-foreground">Loading feed...</div>}>
        <FeedContent />
      </Suspense>
    </FeedLayout>
  )
}
