"use client"

import { useState, useEffect, useMemo } from "react"
import { useSearchParams } from "next/navigation"
import { PostCard } from "./post-card"
import { CreatePostBox } from "./create-post-box"
import { Button } from "@/components/ui/button"
import { Sparkles, Loader2 } from "lucide-react"
import { getFeed, getPost } from "@/lib/api"
import type { Post } from "@/lib/onvm-client"
import { useToast } from "@/hooks/use-toast"
import { getSession } from "@/lib/auth"

type FeedTab = "following" | "discover"

interface FeedContentProps {
  initialTab?: FeedTab
  fixedDiscover?: boolean
  title?: string
  hideToggle?: boolean
}

export function FeedContent({ initialTab = "following", fixedDiscover = false, title, hideToggle = false }: FeedContentProps) {
  const [activeTab, setActiveTab] = useState<FeedTab>(fixedDiscover ? "discover" : initialTab)
  const [posts, setPosts] = useState<Post[]>([])
  const [isLoading, setIsLoading] = useState(true)
  const [unauthenticated, setUnauthenticated] = useState(false)
  const { toast } = useToast()
  const searchParams = useSearchParams()
  const focusedPostId = searchParams.get("post")

  const loadFeed = async () => {
    // If a specific post is requested, fetch it directly (no session required)
    if (focusedPostId) {
      setIsLoading(true)
      setUnauthenticated(false)
      try {
        const p = await getPost(focusedPostId)
        setPosts(p ? [p] : [])
      } catch (error) {
        console.error("[v0] Feed load error:", error)
        toast({
          title: "Failed to load post",
          description: error instanceof Error ? error.message : "Please try again.",
          variant: "destructive",
        })
        setPosts([])
      } finally {
        setIsLoading(false)
      }
      return
    }
    const session = getSession()
    if (!session?.session) {
      setUnauthenticated(true)
      setIsLoading(false)
      return
    }
    setUnauthenticated(false)
    setIsLoading(true)
    try {
      const feedData = await getFeed(20, activeTab === "discover")
      setPosts(feedData)
    } catch (error) {
      console.error("[v0] Feed load error:", error)
      toast({
        title: "Failed to load feed",
        description: error instanceof Error ? error.message : "Please try again.",
        variant: "destructive",
      })
    } finally {
      setIsLoading(false)
    }
  }

  useEffect(() => {
    if (fixedDiscover && activeTab !== "discover") {
      setActiveTab("discover")
      return
    }
    loadFeed()
  }, [activeTab, fixedDiscover, focusedPostId])

  const headerTitle = useMemo(() => {
    if (title) return title
    return fixedDiscover ? "Discover" : "Home"
  }, [fixedDiscover, title])

  return (
    <div className="w-full max-w-2xl mx-auto px-2 sm:px-0">
      <div className="sticky top-0 z-40 bg-background/80 backdrop-blur-xl border-b border-border/50">
        <div className="flex items-center justify-between p-4">
          <h1 className="text-2xl font-bold">{headerTitle}</h1>
          {!hideToggle && (
            <Button
              size="sm"
              variant="outline"
              className={`gap-2 rounded-full ${
                activeTab === "discover" ? "border-primary text-primary" : "text-muted-foreground"
              } hover:text-primary transition-colors`}
              onClick={() => setActiveTab(activeTab === "following" ? "discover" : "following")}
            >
              <Sparkles className="w-4 h-4" />
              {activeTab === "following" ? "Switch to Discover" : "Switch to Following"}
            </Button>
          )}
        </div>

        {!fixedDiscover && (
          <div className="flex border-b border-border/50">
            <button
              className={`flex-1 py-3 text-sm font-medium transition-colors relative ${
                activeTab === "following"
                  ? "text-foreground"
                  : "text-muted-foreground hover:text-foreground hover:bg-secondary/30"
              }`}
              onClick={() => setActiveTab("following")}
            >
              Following
              {activeTab === "following" && (
                <div className="absolute bottom-0 left-0 right-0 h-1 bg-gradient-to-r from-primary to-accent rounded-t-full" />
              )}
            </button>
            <button
              className={`flex-1 py-3 text-sm font-medium transition-colors relative ${
                activeTab === "discover"
                  ? "text-foreground"
                  : "text-muted-foreground hover:text-foreground hover:bg-secondary/30"
              }`}
              onClick={() => setActiveTab("discover")}
            >
              Discover
              {activeTab === "discover" && (
                <div className="absolute bottom-0 left-0 right-0 h-1 bg-gradient-to-r from-primary to-accent rounded-t-full" />
              )}
            </button>
          </div>
        )}
      </div>

      <div className="border-b border-border/50">
        {unauthenticated ? (
          <div className="p-4 text-muted-foreground text-sm">Please log in to create a post.</div>
        ) : (
          <CreatePostBox onPostCreated={loadFeed} />
        )}
      </div>

      <div>
        {unauthenticated ? (
          <div className="text-center py-12 text-muted-foreground">
            <p>Log in to see your feed.</p>
          </div>
        ) : isLoading ? (
          <div className="flex items-center justify-center py-12">
            <Loader2 className="w-8 h-8 animate-spin text-primary" />
          </div>
        ) : posts.length === 0 ? (
          <div className="text-center py-12 text-muted-foreground">
            <p>No posts yet. Be the first to share something!</p>
          </div>
        ) : (
            posts.map((post) => <PostCard key={post.id} post={post} onUpdate={loadFeed} />)
        )}
      </div>
    </div>
  )
}
