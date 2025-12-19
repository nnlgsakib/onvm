"use client"

import { useEffect, useMemo, useState } from "react"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import { Heart, MessageCircle, Share, Bookmark, MoreHorizontal } from "lucide-react"
import { likePost, fetchBlob, commentOnPost, deletePost } from "@/lib/api"
import { useToast } from "@/hooks/use-toast"
import type { Post } from "@/lib/onvm-client"
import { getCurrentUsername } from "@/lib/auth"
import Link from "next/link"
import { Avatar as UIAvatar, AvatarFallback as UIAvatarFallback } from "@/components/ui/avatar"

interface PostCardProps {
  post: Post
  onUpdate?: () => void
}

export function PostCard({ post, onUpdate }: PostCardProps) {
  const [isLiking, setIsLiking] = useState(false)
  const [bookmarked, setBookmarked] = useState(false)
  const [attachments, setAttachments] = useState<Record<string, string>>({})
  const [attachmentErrors, setAttachmentErrors] = useState<Record<string, string>>({})
  const [showCommentBox, setShowCommentBox] = useState(false)
  const [commentText, setCommentText] = useState("")
  const [isCommenting, setIsCommenting] = useState(false)
  const currentUser = getCurrentUsername()
  const { toast } = useToast()

  const handleLike = async () => {
    if (isLiking) return

    setIsLiking(true)
    try {
      await likePost(post.id)

      if (onUpdate) {
        onUpdate()
      }
    } catch (error) {
      console.error("[v0] Like error:", error)
      toast({
        title: "Failed to like",
        description: error instanceof Error ? error.message : "Please try again.",
        variant: "destructive",
      })
    } finally {
      setIsLiking(false)
    }
  }

  const initials = useMemo(() => {
    if (!post || !post.author) return "NB"
    return post.author.substring(0, 2).toUpperCase()
  }, [post?.author])

  const formatTimestamp = (timestamp: number) => {
    const now = Date.now()
    const diff = now - timestamp
    const minutes = Math.floor(diff / 60000)
    const hours = Math.floor(diff / 3600000)
    const days = Math.floor(diff / 86400000)

    if (minutes < 1) return "just now"
    if (minutes < 60) return `${minutes}m ago`
    if (hours < 24) return `${hours}h ago`
    return `${days}d ago`
  }

  useEffect(() => {
    let cancelled = false
    const load = async () => {
      if (!post?.attachments?.length) return
      const next: Record<string, string> = {}
      const errs: Record<string, string> = {}
      for (const att of post.attachments) {
        try {
          const { blob, contentType } = await fetchBlob(att.blob_id_hex)
          if (cancelled) return
          next[att.blob_id_hex] = `${URL.createObjectURL(blob)}|${contentType || "application/octet-stream"}`
        } catch (e) {
          errs[att.blob_id_hex] = e instanceof Error ? e.message : "failed to load"
        }
      }
      if (!cancelled) {
        setAttachments(next)
        setAttachmentErrors(errs)
      }
    }
    load()
    return () => {
      cancelled = true
      Object.values(attachments).forEach((u) => URL.revokeObjectURL(u.split("|")[0]))
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [post?.id, post?.attachments])

  const handleComment = async () => {
    if (!commentText.trim()) return
    setIsCommenting(true)
    try {
      await commentOnPost(post.id, commentText.trim())
      setCommentText("")
      setShowCommentBox(false)
      onUpdate?.()
    } catch (e) {
      toast({
        title: "Failed to comment",
        description: e instanceof Error ? e.message : "Please try again.",
        variant: "destructive",
      })
    } finally {
      setIsCommenting(false)
    }
  }

  return (
    <Card className="border-x-0 border-t-0 border-b border-border/50 rounded-none bg-transparent hover:bg-card/30 transition-colors group">
      <div className="p-4">
            <div className="flex items-start gap-3">
              <Avatar className="w-10 h-10 border-2 border-transparent group-hover:border-primary/20 transition-colors">
                <AvatarFallback className="bg-gradient-to-br from-primary/20 to-accent/20">
                  {initials}
                </AvatarFallback>
              </Avatar>

          <div className="flex-1 min-w-0">
              <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Link
                  href={`/profile?user=${encodeURIComponent(post?.author ?? "")}`}
                  className="font-semibold hover:text-primary transition-colors cursor-pointer"
                >
                  {post?.author ?? "unknown"}
                </Link>
                <span className="text-sm text-muted-foreground">
                  · {formatTimestamp(post?.createdAt ?? 0)}
                </span>
              </div>
              <Button size="sm" variant="ghost" className="opacity-0 group-hover:opacity-100 transition-opacity">
                <MoreHorizontal className="w-4 h-4" />
              </Button>
            </div>

            <p className="mt-2 text-foreground leading-relaxed whitespace-pre-wrap">
              {post?.text ?? ""}
            </p>

            {post?.attachments?.length ? (
              <div className="mt-3 space-y-3">
                {post.attachments.map((att) => {
                  const stored = attachments[att.blob_id_hex]
                  const url = stored ? stored.split("|")[0] : undefined
                  const ct = stored ? stored.split("|")[1] : ""
                  const err = attachmentErrors[att.blob_id_hex]
                  return (
                    <div key={att.blob_id_hex} className="rounded-md border border-border/60 p-2">
                      <div className="text-xs text-muted-foreground mb-1">
                        {att.blob_id_hex} ({Math.round(att.size / 1024)} KB)
                      </div>
                      {err ? (
                        <div className="text-xs text-destructive">Failed to load: {err}</div>
                      ) : url ? (
                        ct.startsWith("image/") ? (
                          <img
                            src={url}
                            alt="attachment"
                            className="max-h-80 w-full object-contain rounded-md bg-muted/30"
                          />
                        ) : ct.startsWith("audio/") ? (
                          <audio controls className="w-full">
                            <source src={url} type={ct} />
                            Your browser does not support the audio element.
                          </audio>
                        ) : ct.startsWith("video/") ? (
                          <video controls className="w-full max-h-96 rounded-md bg-black">
                            <source src={url} type={ct} />
                            Your browser does not support the video tag.
                          </video>
                        ) : (
                          <a
                            href={url}
                            download
                            className="text-sm text-primary hover:underline"
                            target="_blank"
                            rel="noreferrer"
                          >
                            Download attachment ({ct || "file"})
                          </a>
                        )
                      ) : (
                        <div className="text-xs text-muted-foreground">Loading attachment…</div>
                      )}
                    </div>
                  )
                })}
              </div>
            ) : null}

            <div className="flex items-center justify-between mt-4 pt-2">
              <Button
                size="sm"
                variant="ghost"
                onClick={handleLike}
                disabled={isLiking}
                className="gap-2 text-muted-foreground hover:text-red-500 hover:bg-red-500/10 transition-all hover:scale-110"
              >
                <Heart className="w-4 h-4" />
                <span className="text-sm">{post?.likes ?? 0}</span>
              </Button>

              <Button
                size="sm"
                variant="ghost"
                onClick={() => setShowCommentBox((v) => !v)}
                className="gap-2 text-muted-foreground hover:text-primary hover:bg-primary/10 transition-all hover:scale-110"
              >
                <MessageCircle className="w-4 h-4" />
                <span className="text-sm">{post?.comments?.length ?? 0}</span>
              </Button>

              <Button
                size="sm"
                variant="ghost"
                className="gap-2 text-muted-foreground hover:text-accent hover:bg-accent/10 transition-all hover:scale-110"
              >
                <Share className="w-4 h-4" />
              </Button>

              {currentUser && currentUser === post?.author && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={async () => {
                    try {
                      await deletePost(post.id)
                      toast({ title: "Post deleted" })
                      onUpdate?.()
                    } catch (e) {
                      toast({
                        title: "Delete failed",
                        description: e instanceof Error ? e.message : "Please try again.",
                        variant: "destructive",
                      })
                    }
                  }}
                  className="gap-2 text-destructive hover:bg-destructive/10 transition-all hover:scale-110"
                >
                  Delete
                </Button>
              )}

              <Button
                size="sm"
                variant="ghost"
                onClick={() => setBookmarked(!bookmarked)}
                className={`transition-all hover:scale-110 ${
                  bookmarked
                    ? "text-accent hover:text-accent/80 hover:bg-accent/10"
                    : "text-muted-foreground hover:text-accent hover:bg-accent/10"
                }`}
              >
                <Bookmark className={`w-4 h-4 ${bookmarked ? "fill-current" : ""}`} />
              </Button>
            </div>

            {showCommentBox && (
              <div className="mt-3 space-y-2">
                <textarea
                  value={commentText}
                  onChange={(e) => setCommentText(e.target.value)}
                  className="w-full rounded-md border border-border/60 bg-background px-3 py-2 text-sm"
                  placeholder="Add a comment"
                  maxLength={1024}
                />
                <div className="flex justify-end gap-2">
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      setCommentText("")
                      setShowCommentBox(false)
                    }}
                  >
                    Cancel
                  </Button>
                  <Button size="sm" onClick={handleComment} disabled={isCommenting || !commentText.trim()}>
                    {isCommenting ? "Posting..." : "Post"}
                  </Button>
                </div>
              </div>
            )}

            {post?.comments?.length ? (
              <div className="mt-4 space-y-2">
                {post.comments.map((c) => (
                  <div key={c.id} className="text-sm border-t border-border/40 pt-2">
                    <span className="font-semibold">{c.author}</span>:{" "}
                    <span className="text-muted-foreground">{c.text}</span>
                  </div>
                ))}
              </div>
            ) : null}
          </div>
        </div>
      </div>
    </Card>
  )
}
