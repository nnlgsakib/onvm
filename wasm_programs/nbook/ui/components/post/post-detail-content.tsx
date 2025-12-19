"use client"

import { useState } from "react"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { Card } from "@/components/ui/card"
import { Heart, MessageCircle, Share, Bookmark, ArrowLeft, Send } from "lucide-react"
import Link from "next/link"

export function PostDetailContent({ postId }: { postId: string }) {
  const [liked, setLiked] = useState(false)
  const [likeCount, setLikeCount] = useState(234)
  const [bookmarked, setBookmarked] = useState(false)
  const [comment, setComment] = useState("")

  // Mock post data - you'll fetch this from your backend
  const post = {
    id: postId,
    author: "sarah_dev",
    authorAvatar: "SD",
    text: "Just deployed my first Next.js app to Vercel! The developer experience is absolutely incredible. From local dev to production in minutes. The Edge Runtime is blazing fast and the analytics are super helpful. Can't wait to build more! 🚀",
    timestamp: "2 hours ago",
  }

  const comments = [
    {
      id: "1",
      author: "mike_design",
      authorAvatar: "MD",
      text: "Congrats! Vercel is amazing. What kind of app did you build?",
      timestamp: "1 hour ago",
      likes: 12,
    },
    {
      id: "2",
      author: "alex_ai",
      authorAvatar: "AA",
      text: "Welcome to the Vercel community! The platform just keeps getting better.",
      timestamp: "45 minutes ago",
      likes: 8,
    },
  ]

  const handleLike = () => {
    setLiked(!liked)
    setLikeCount(liked ? likeCount - 1 : likeCount + 1)
  }

  const handleComment = () => {
    // Handle comment submission - integrate with your backend
    console.log("Comment:", comment)
    setComment("")
  }

  return (
    <div className="max-w-2xl mx-auto">
      {/* Header */}
      <div className="sticky top-0 z-40 bg-background/80 backdrop-blur-xl border-b border-border/50">
        <div className="flex items-center gap-4 p-4">
          <Link href="/feed">
            <Button size="sm" variant="ghost" className="hover:bg-secondary/50">
              <ArrowLeft className="w-4 h-4" />
            </Button>
          </Link>
          <h1 className="text-xl font-bold">Post</h1>
        </div>
      </div>

      {/* Post */}
      <Card className="border-x-0 border-t-0 border-b border-border/50 rounded-none bg-transparent">
        <div className="p-4">
          <div className="flex items-start gap-3 mb-4">
            <Avatar className="w-12 h-12 border-2 border-primary/20">
              <AvatarFallback className="bg-gradient-to-br from-primary/20 to-accent/20">
                {post.authorAvatar}
              </AvatarFallback>
            </Avatar>
            <div>
              <div className="font-semibold">{post.author}</div>
              <div className="text-sm text-muted-foreground">{post.timestamp}</div>
            </div>
          </div>

          <p className="text-lg leading-relaxed whitespace-pre-wrap mb-6">{post.text}</p>

          {/* Stats */}
          <div className="flex items-center gap-6 py-3 border-y border-border/50 text-sm text-muted-foreground">
            <div>
              <span className="font-bold text-foreground">{likeCount}</span> Likes
            </div>
            <div>
              <span className="font-bold text-foreground">{comments.length}</span> Comments
            </div>
          </div>

          {/* Actions */}
          <div className="flex items-center justify-around py-3">
            <Button
              size="lg"
              variant="ghost"
              onClick={handleLike}
              className={`gap-2 transition-all hover:scale-110 ${
                liked
                  ? "text-red-500 hover:text-red-600 hover:bg-red-500/10"
                  : "text-muted-foreground hover:text-red-500 hover:bg-red-500/10"
              }`}
            >
              <Heart className={`w-5 h-5 ${liked ? "fill-current" : ""}`} />
            </Button>

            <Button
              size="lg"
              variant="ghost"
              className="gap-2 text-muted-foreground hover:text-primary hover:bg-primary/10 transition-all hover:scale-110"
            >
              <MessageCircle className="w-5 h-5" />
            </Button>

            <Button
              size="lg"
              variant="ghost"
              className="gap-2 text-muted-foreground hover:text-accent hover:bg-accent/10 transition-all hover:scale-110"
            >
              <Share className="w-5 h-5" />
            </Button>

            <Button
              size="lg"
              variant="ghost"
              onClick={() => setBookmarked(!bookmarked)}
              className={`transition-all hover:scale-110 ${
                bookmarked
                  ? "text-accent hover:text-accent/80 hover:bg-accent/10"
                  : "text-muted-foreground hover:text-accent hover:bg-accent/10"
              }`}
            >
              <Bookmark className={`w-5 h-5 ${bookmarked ? "fill-current" : ""}`} />
            </Button>
          </div>
        </div>
      </Card>

      {/* Add Comment */}
      <Card className="border-x-0 border-t-0 border-b border-border/50 rounded-none bg-transparent p-4">
        <div className="flex gap-3">
          <Avatar className="w-10 h-10 border-2 border-primary/20">
            <AvatarFallback className="bg-gradient-to-br from-primary to-accent text-primary-foreground">
              JD
            </AvatarFallback>
          </Avatar>
          <div className="flex-1">
            <Textarea
              placeholder="Write a comment..."
              value={comment}
              onChange={(e) => setComment(e.target.value)}
              className="min-h-[80px] resize-none bg-transparent border-border/50 focus:border-primary transition-colors"
            />
            {comment && (
              <Button
                onClick={handleComment}
                size="sm"
                className="mt-2 bg-gradient-to-r from-primary to-accent hover:opacity-90 transition-all hover:scale-105"
              >
                <Send className="w-4 h-4 mr-1" />
                Comment
              </Button>
            )}
          </div>
        </div>
      </Card>

      {/* Comments */}
      <div>
        {comments.map((comment) => (
          <Card
            key={comment.id}
            className="border-x-0 border-t-0 border-b border-border/50 rounded-none bg-transparent hover:bg-card/30 transition-colors"
          >
            <div className="p-4">
              <div className="flex gap-3">
                <Avatar className="w-10 h-10 border-2 border-transparent">
                  <AvatarFallback className="bg-gradient-to-br from-primary/20 to-accent/20">
                    {comment.authorAvatar}
                  </AvatarFallback>
                </Avatar>
                <div className="flex-1">
                  <div className="flex items-center gap-2 mb-1">
                    <span className="font-semibold text-sm">{comment.author}</span>
                    <span className="text-xs text-muted-foreground">· {comment.timestamp}</span>
                  </div>
                  <p className="text-sm leading-relaxed">{comment.text}</p>
                  <Button
                    size="sm"
                    variant="ghost"
                    className="mt-2 gap-1 text-muted-foreground hover:text-red-500 hover:bg-red-500/10"
                  >
                    <Heart className="w-3 h-3" />
                    <span className="text-xs">{comment.likes}</span>
                  </Button>
                </div>
              </div>
            </div>
          </Card>
        ))}
      </div>
    </div>
  )
}
