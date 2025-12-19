import { FeedLayout } from "@/components/feed/feed-layout"
import { PostDetailContent } from "@/components/post/post-detail-content"

export default function PostDetailPage({ params }: { params: { id: string } }) {
  return (
    <FeedLayout>
      <PostDetailContent postId={params.id} />
    </FeedLayout>
  )
}
