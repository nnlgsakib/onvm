import { FeedLayout } from "@/components/feed/feed-layout"
import { ProfileContent } from "@/components/profile/profile-content"
import { Suspense } from "react"

export default function ProfilePage() {
  return (
    <FeedLayout>
      <Suspense fallback={<div className="p-6 text-muted-foreground">Loading profile...</div>}>
        <ProfileContent />
      </Suspense>
    </FeedLayout>
  )
}
