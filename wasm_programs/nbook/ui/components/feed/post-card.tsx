"use client"

import { useEffect, useMemo, useState, MouseEvent } from "react"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import { Heart, MessageCircle, Share, Bookmark, MoreHorizontal } from "lucide-react"
import {
  likePost,
  fetchBlob,
  commentOnPost,
  deletePost,
  followUser,
  unfollowUser,
  listFollowing,
  getProfile,
} from "@/lib/api"
import { useToast } from "@/hooks/use-toast"
import type { Post } from "@/lib/onvm-client"
import { getCurrentUsername } from "@/lib/auth"
import Link from "next/link"

let cachedFollowing: Set<string> | null = null
let cachedFollowingPromise: Promise<Set<string>> | null = null

async function getFollowingSet(username: string): Promise<Set<string>> {
  if (cachedFollowing) return cachedFollowing
  if (cachedFollowingPromise) return cachedFollowingPromise
  cachedFollowingPromise = listFollowing(username, 500)
    .then((users) => {
      cachedFollowing = new Set(users.map((u) => u.username))
      return cachedFollowing
    })
    .catch((err) => {
      cachedFollowingPromise = null
      throw err
    })
  return cachedFollowingPromise
}

interface PostCardProps {
  post: Post
  onUpdate?: () => void
  disableModal?: boolean
}

export function PostCard({ post, onUpdate, disableModal }: PostCardProps) {
  const [localPost, setLocalPost] = useState<Post>(post)
  const [isLiking, setIsLiking] = useState(false)
  const [bookmarked, setBookmarked] = useState(false)
  const [attachments, setAttachments] = useState<Record<string, string>>({})
  const [attachmentErrors, setAttachmentErrors] = useState<Record<string, string>>({})
  const [showCommentBox, setShowCommentBox] = useState(false)
  const [commentText, setCommentText] = useState("")
  const [isCommenting, setIsCommenting] = useState(false)
  const currentUser = getCurrentUsername()
  const { toast } = useToast()
  const [sharing, setSharing] = useState(false)
  const [shareOpen, setShareOpen] = useState(false)
  const [isFollowingAuthor, setIsFollowingAuthor] = useState<boolean | null>(null)
  const [followBusy, setFollowBusy] = useState(false)
  const [detailOpen, setDetailOpen] = useState(false)
  const shareUrl = useMemo(() => {
    const base = typeof window !== "undefined" ? window.location.origin : ""
    return `${base}/feed?post=${encodeURIComponent(localPost?.id ?? "")}`
  }, [localPost?.id])

  const handleLike = async (e?: MouseEvent) => {
    e?.stopPropagation()
    if (isLiking) return

    setIsLiking(true)
    try {
      const updated = await likePost(localPost.id)
      setLocalPost(updated)
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
    const fallback = "NB"
    const author = localPost?.author || post?.author
    if (!author) return fallback
    return author.substring(0, 2).toUpperCase()
  }, [localPost?.author, post?.author])

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
      if (!localPost?.attachments?.length) return
      const next: Record<string, string> = {}
      const errs: Record<string, string> = {}
      for (const att of localPost.attachments) {
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
  }, [localPost?.id, localPost?.attachments])

  useEffect(() => {
    setLocalPost(post)
  }, [post])

  useEffect(() => {
    let cancelled = false
    const loadAuthorProfile = async () => {
      if (!localPost?.author || localPost.avatarBlobId) return
      try {
        const p = await getProfile(localPost.author)
        if (!cancelled && p.avatarBlobId) {
          setLocalPost((prev) => (prev ? { ...prev, avatarBlobId: p.avatarBlobId } : prev))
        }
      } catch (e) {
        console.warn("Failed to load author profile", e)
      }
    }
    loadAuthorProfile()
    return () => {
      cancelled = true
    }
  }, [localPost?.author, localPost?.avatarBlobId])

  useEffect(() => {
    let cancelled = false
    const loadFollowState = async () => {
      if (!currentUser || !localPost?.author || currentUser === localPost.author) {
        setIsFollowingAuthor(false)
        return
      }
      try {
        const following = await getFollowingSet(currentUser)
        if (!cancelled) {
          setIsFollowingAuthor(following.has(localPost.author))
        }
      } catch (error) {
        if (!cancelled) {
          setIsFollowingAuthor(false)
        }
        console.warn("Failed to load following list", error)
      }
    }
    loadFollowState()
    return () => {
      cancelled = true
    }
  }, [currentUser, localPost?.author])

  const handleComment = async (e?: MouseEvent) => {
    e?.stopPropagation()
    if (!commentText.trim()) return
    setIsCommenting(true)
    try {
      const updated = await commentOnPost(localPost.id, commentText.trim())
      setLocalPost(updated)
      setCommentText("")
      setShowCommentBox(false)
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

  const handleFollowToggle = async (e?: MouseEvent) => {
    e?.stopPropagation()
    if (!currentUser || !localPost?.author || currentUser === localPost.author) {
      toast({
        title: "Please log in",
        description: "Sign in to follow users.",
      })
      return
    }
    if (isFollowingAuthor === null) return
    setFollowBusy(true)
    try {
      const following = await getFollowingSet(currentUser)
      if (isFollowingAuthor) {
        await unfollowUser(localPost.author)
        following.delete(localPost.author)
        setIsFollowingAuthor(false)
        toast({ title: `Unfollowed ${localPost.author}` })
      } else {
        await followUser(localPost.author)
        following.add(localPost.author)
        setIsFollowingAuthor(true)
        toast({ title: `Following ${localPost.author}` })
      }
    } catch (error) {
      toast({
        title: "Follow action failed",
        description: error instanceof Error ? error.message : "Please try again.",
        variant: "destructive",
      })
    } finally {
      setFollowBusy(false)
    }
  }

  return (
    <>
      <Card
        className="border border-border/40 rounded-2xl bg-card/70 backdrop-blur-md hover:-translate-y-1 hover:shadow-lg transition-all group w-full cursor-pointer"
        onClick={() => {
          if (!disableModal) setDetailOpen(true)
        }}
      >
        <div className="p-4">
          <div className="flex items-start gap-3">
        <Avatar className="w-10 h-10 border-2 border-transparent group-hover:border-primary/20 transition-colors">
          {localPost.avatarBlobId ? (
            <img
              src={`/api/onvm/blob/${localPost.avatarBlobId}`}
              alt={localPost.author}
              className="w-full h-full object-cover rounded-full"
              onError={(e) => {
                const target = e.currentTarget
                target.style.display = "none"
              }}
            />
          ) : null}
          <AvatarFallback className="bg-gradient-to-br from-primary/20 to-accent/20">
            {initials}
          </AvatarFallback>
        </Avatar>

          <div className="flex-1 min-w-0">
              <div className="flex items-center justify-between gap-2 flex-wrap">
                <div className="flex items-center gap-2">
                  <Link
                    href={`/profile?user=${encodeURIComponent(localPost?.author ?? "")}`}
                    className="font-semibold hover:text-primary transition-colors cursor-pointer"
                    onClick={(e) => e.stopPropagation()}
                  >
                    {localPost?.author ?? "unknown"}
                  </Link>
                  <span className="text-sm text-muted-foreground">
                    · {formatTimestamp(localPost?.createdAt ?? 0)}
                  </span>
                </div>
                <div className="flex items-center gap-2">
                  {currentUser && localPost?.author && currentUser !== localPost.author ? (
                    <Button
                      size="sm"
                      variant={isFollowingAuthor ? "outline" : "default"}
                    onClick={handleFollowToggle}
                    disabled={followBusy}
                    className="h-8"
                  >
                    {followBusy ? "..." : isFollowingAuthor ? "Unfollow" : "Follow"}
                  </Button>
                ) : null}
                <Button
                  size="sm"
                  variant="ghost"
                  className="opacity-0 group-hover:opacity-100 transition-opacity"
                  onClick={(e) => e.stopPropagation()}
                >
                  <MoreHorizontal className="w-4 h-4" />
                </Button>
              </div>
            </div>

            <p className="mt-2 text-foreground leading-relaxed whitespace-pre-wrap break-words">
              {post?.text ?? ""}
            </p>

            {localPost?.attachments?.length ? (
              <div className="mt-3 space-y-3">
                {localPost.attachments.map((att) => {
                  const stored = attachments[att.blob_id_hex]
                  const url = stored ? stored.split("|")[0] : undefined
                  const ct = stored ? stored.split("|")[1] : ""
                  const err = attachmentErrors[att.blob_id_hex]
                  const shortId =
                    att.blob_id_hex.length > 16
                      ? `${att.blob_id_hex.slice(0, 8)}…${att.blob_id_hex.slice(-6)}`
                      : att.blob_id_hex
                  return (
                    <div key={att.blob_id_hex} className="rounded-md border border-border/60 p-2">
                      <div className="text-xs text-muted-foreground mb-1 flex items-center gap-2 flex-wrap">
                        <span className="font-mono break-all">{shortId}</span>
                        <span>({Math.round(att.size / 1024)} KB)</span>
                        <Button
                          size="sm"
                          variant="ghost"
                          className="h-6 px-2 text-xs"
                          onClick={async (e) => {
                            e.stopPropagation()
                            try {
                              await navigator.clipboard.writeText(att.blob_id_hex)
                              toast({ title: "Attachment ID copied" })
                            } catch {
                              toast({
                                title: "Copy failed",
                                description: "Could not copy attachment id.",
                                variant: "destructive",
                              })
                            }
                          }}
                        >
                          Copy
                        </Button>
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
                <span className="text-sm">{localPost?.likes ?? 0}</span>
              </Button>

              <Button
                size="sm"
                variant="ghost"
                onClick={(e) => {
                  e.stopPropagation()
                  setShowCommentBox((v) => !v)
                }}
                className="gap-2 text-muted-foreground hover:text-primary hover:bg-primary/10 transition-all hover:scale-110"
              >
                <MessageCircle className="w-4 h-4" />
                <span className="text-sm">{localPost?.comments?.length ?? 0}</span>
              </Button>

              <Button
                size="sm"
                variant="ghost"
                onClick={(e) => {
                  e.stopPropagation()
                  setShareOpen(true)
                }}
                className="gap-2 text-muted-foreground hover:text-accent hover:bg-accent/10 transition-all hover:scale-110"
              >
                <Share className="w-4 h-4" />
              </Button>

              {currentUser && currentUser === localPost?.author && (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={async (e) => {
                    e.stopPropagation()
                    try {
                      await deletePost(localPost.id)
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
                onClick={(e) => {
                  e.stopPropagation()
                  setBookmarked(!bookmarked)
                }}
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
              <div
                className="mt-3 space-y-2"
                onClick={(e) => e.stopPropagation()}
              >
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

            {localPost?.comments?.length ? (
              <div className="mt-4 space-y-2">
                {localPost.comments.map((c) => (
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
      {detailOpen && (
        <div
          className="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm flex items-center justify-center px-4"
          onClick={() => setDetailOpen(false)}
        >
          <div
            className="bg-background rounded-2xl shadow-2xl max-w-3xl w-full max-h-[90vh] overflow-y-auto p-4 space-y-4"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-start justify-between gap-3">
              <div className="flex items-center gap-3">
                <Avatar className="w-12 h-12 border-2 border-primary/20">
                  {localPost.avatarBlobId ? (
                    <img
                      src={`/api/onvm/blob/${localPost.avatarBlobId}`}
                      alt={localPost.author}
                      className="w-full h-full object-cover rounded-full"
                    />
                  ) : null}
                  <AvatarFallback className="bg-gradient-to-br from-primary/20 to-accent/20 text-lg">
                    {initials}
                  </AvatarFallback>
                </Avatar>
                <div>
                  <div className="font-semibold text-lg">{localPost?.author ?? "unknown"}</div>
                  <div className="text-sm text-muted-foreground">
                    {formatTimestamp(localPost?.createdAt ?? 0)}
                  </div>
                </div>
              </div>
              <div className="flex items-center gap-2">
                {currentUser && localPost?.author && currentUser !== localPost.author ? (
                  <Button
                    size="sm"
                    variant={isFollowingAuthor ? "outline" : "default"}
                    onClick={handleFollowToggle}
                    disabled={followBusy}
                    className="h-9"
                  >
                    {followBusy ? "..." : isFollowingAuthor ? "Unfollow" : "Follow"}
                  </Button>
                ) : null}
                <Button size="sm" variant="ghost" onClick={() => setDetailOpen(false)}>
                  Close
                </Button>
              </div>
            </div>

            <p className="text-lg leading-relaxed whitespace-pre-wrap break-words">{localPost?.text ?? ""}</p>

            {localPost?.attachments?.length ? (
              <div className="space-y-3">
                {localPost.attachments.map((att) => {
                  const stored = attachments[att.blob_id_hex]
                  const url = stored ? stored.split("|")[0] : undefined
                  const ct = stored ? stored.split("|")[1] : ""
                  const err = attachmentErrors[att.blob_id_hex]
                  const shortId =
                    att.blob_id_hex.length > 16
                      ? `${att.blob_id_hex.slice(0, 8)}…${att.blob_id_hex.slice(-6)}`
                      : att.blob_id_hex
                  return (
                    <div key={att.blob_id_hex} className="rounded-md border border-border/60 p-2">
                      <div className="text-xs text-muted-foreground mb-1 flex items-center gap-2 flex-wrap">
                        <span className="font-mono break-all">{shortId}</span>
                        <span>({Math.round(att.size / 1024)} KB)</span>
                        <Button
                          size="sm"
                          variant="ghost"
                          className="h-6 px-2 text-xs"
                          onClick={async (e) => {
                            e.stopPropagation()
                            try {
                              await navigator.clipboard.writeText(att.blob_id_hex)
                              toast({ title: "Attachment ID copied" })
                            } catch {
                              toast({
                                title: "Copy failed",
                                description: "Could not copy attachment id.",
                                variant: "destructive",
                              })
                            }
                          }}
                        >
                          Copy
                        </Button>
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
                        <div className="text-xs text-muted-foreground">Loading attachment...</div>
                      )}
                    </div>
                  )
                })}
              </div>
            ) : null}

            <div className="flex items-center gap-6 text-sm text-muted-foreground">
              <div className="flex items-center gap-2">
                <Heart className="w-4 h-4" />
                <span className="font-semibold text-foreground">{localPost?.likes ?? 0}</span>
              </div>
              <div className="flex items-center gap-2">
                <MessageCircle className="w-4 h-4" />
                <span className="font-semibold text-foreground">{localPost?.comments?.length ?? 0}</span>
              </div>
            </div>

            <div className="flex items-center justify-around py-2 border border-border/50 rounded-xl">
              <Button
                size="sm"
                variant="ghost"
                onClick={handleLike}
                className={`gap-2 transition-all hover:scale-110 ${
                  isLiking ? "opacity-60" : ""
                }`}
                disabled={isLiking}
              >
                <Heart className={`w-4 h-4 ${isLiking ? "" : ""}`} />
                <span className="text-sm">{localPost?.likes ?? 0}</span>
              </Button>

              <Button
                size="sm"
                variant="ghost"
                onClick={(e) => {
                  e.stopPropagation()
                  setShowCommentBox(true)
                }}
                className="gap-2 text-muted-foreground hover:text-primary hover:bg-primary/10 transition-all hover:scale-110"
              >
                <MessageCircle className="w-4 h-4" />
                <span className="text-sm">{localPost?.comments?.length ?? 0}</span>
              </Button>

              <Button
                size="sm"
                variant="ghost"
                onClick={(e) => {
                  e.stopPropagation()
                  setShareOpen(true)
                }}
                className="gap-2 text-muted-foreground hover:text-accent hover:bg-accent/10 transition-all hover:scale-110"
              >
                <Share className="w-4 h-4" />
              </Button>
            </div>

            <div className="space-y-3">
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
                  onClick={(e) => {
                    e.stopPropagation()
                    setCommentText("")
                  }}
                >
                  Clear
                </Button>
                <Button size="sm" onClick={handleComment} disabled={isCommenting || !commentText.trim()}>
                  {isCommenting ? "Posting..." : "Post"}
                </Button>
              </div>
            </div>

            {localPost?.comments?.length ? (
              <div className="space-y-3">
                {localPost.comments.map((c) => (
                  <div key={c.id} className="rounded-lg border border-border/50 p-3">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="font-semibold">{c.author}</span>
                      <span className="text-xs text-muted-foreground">· {formatTimestamp(c.createdAt)}</span>
                    </div>
                    <p className="text-sm text-foreground">{c.text}</p>
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-sm text-muted-foreground">No comments yet.</div>
            )}
          </div>
        </div>
      )}
      {shareOpen && (
        <div
          className="fixed inset-0 z-50 bg-black/50 backdrop-blur-sm flex items-center justify-center px-4"
          onClick={() => setShareOpen(false)}
        >
          <div
            className="bg-background rounded-lg shadow-lg max-w-sm w-full p-4 space-y-3"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="flex items-center justify-between">
              <h3 className="text-lg font-semibold">Share post</h3>
              <Button variant="ghost" size="sm" onClick={() => setShareOpen(false)}>
                Close
              </Button>
            </div>
            <div className="space-y-2">
              <Button
                variant="outline"
                className="w-full justify-start gap-2"
                onClick={async () => {
                  try {
                    await navigator.clipboard.writeText(shareUrl)
                    toast({ title: "Link copied" })
                  } catch (e) {
                    toast({
                      title: "Copy failed",
                      description: e instanceof Error ? e.message : "Please try again.",
                      variant: "destructive",
                    })
                  }
                }}
              >
                Copy link
              </Button>
              <Button
                variant="outline"
                className="w-full justify-start gap-2"
                disabled={sharing}
                onClick={async () => {
                  setSharing(true)
                  try {
                    await navigator.share({
                      title: "Check this post",
                      text: localPost?.text,
                      url: shareUrl,
                    })
                  } catch (e) {
                    if (!(e instanceof Error && e.name === "AbortError")) {
                      toast({
                        title: "Share failed",
                        description: e instanceof Error ? e.message : "Please try again.",
                        variant: "destructive",
                      })
                    }
                  } finally {
                    setSharing(false)
                  }
                }}
              >
                Share via device...
              </Button>
            </div>
          </div>
        </div>
      )}
    </>
  )
}
