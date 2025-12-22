"use client"

import { useEffect, useState } from "react"
import { Card } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Search, TrendingUp, UserPlus } from "lucide-react"
import { getSuggestions, searchUsers, followUser } from "@/lib/api"
import { getSession } from "@/lib/auth"
import type { UserSummary } from "@/lib/onvm-client"
import { useToast } from "@/hooks/use-toast"

export function RightPanel() {
  const [query, setQuery] = useState("")
  const [results, setResults] = useState<UserSummary[]>([])
  const [suggestions, setSuggestions] = useState<UserSummary[]>([])
  const [pendingFollow, setPendingFollow] = useState<string | null>(null)
  const { toast } = useToast()
  const session = getSession()

  useEffect(() => {
    const loadSuggestions = async () => {
      if (!session?.session) return
      try {
        const users = await getSuggestions(5)
        setSuggestions(users)
      } catch (e) {
        console.error("suggestions failed", e)
      }
    }
    loadSuggestions()
  }, [session?.session])

  const performSearch = async () => {
    if (!query.trim()) return
    if (!session?.session) {
      toast({
        title: "Login required",
        description: "Log in to search users.",
        variant: "destructive",
      })
      return
    }
    try {
      const users = await searchUsers(query.trim(), 10)
      setResults(users)
    } catch (e) {
      toast({
        title: "Search failed",
        description: e instanceof Error ? e.message : "Please try again.",
        variant: "destructive",
      })
    }
  }

  const renderUserRow = (user: UserSummary, action?: React.ReactNode) => (
    <a
      key={user.username}
      href={`/profile?user=${encodeURIComponent(user.username)}`}
      className="flex items-center gap-3 group"
    >
      <Avatar className="w-10 h-10 border-2 border-transparent group-hover:border-primary/30 transition-colors">
        {user.avatarBlobId ? (
          <img
            src={`/api/onvm/blob/${user.avatarBlobId}`}
            alt={user.username}
            className="w-full h-full object-cover rounded-full"
            onError={(e) => {
              const target = e.currentTarget
              target.style.display = "none"
            }}
          />
        ) : null}
        <AvatarFallback className="bg-gradient-to-br from-primary/20 to-accent/20">
          {(user.displayName || user.username || "NB").substring(0, 2).toUpperCase()}
        </AvatarFallback>
      </Avatar>
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium truncate group-hover:text-primary transition-colors">
          {user.displayName || user.username}
        </div>
        <div className="text-xs text-muted-foreground truncate">{user.bio || "No bio yet"}</div>
        <div className="text-xs text-muted-foreground">{user.followers} followers</div>
      </div>
      {action}
    </a>
  )

  return (
    <div className="p-4 space-y-4">
      <div className="sticky top-4 z-10 space-y-3">
        <div className="relative group">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground group-focus-within:text-primary transition-colors" />
          <Input
            placeholder="Search users..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") performSearch()
            }}
            className="pl-10 bg-background/50 border-border/50 focus:border-primary transition-colors"
          />
        </div>
        {results.length > 0 && (
          <Card className="p-3 space-y-3 bg-card/60 border-border/50">
            {results.map((user) => renderUserRow(user))}
          </Card>
        )}
      </div>

      <Card className="p-4 bg-card/50 border-border/50 backdrop-blur-sm">
        <div className="flex items-center gap-2 mb-4">
          <TrendingUp className="w-5 h-5 text-primary" />
          <h3 className="font-semibold">Discover people</h3>
        </div>
        {suggestions.length === 0 ? (
          <div className="text-sm text-muted-foreground">Sign in to see suggestions.</div>
        ) : (
          <div className="space-y-3">
            {suggestions.map((user) =>
              renderUserRow(
                user,
                <Button
                  size="sm"
                  variant="outline"
                  disabled={pendingFollow === user.username}
                  onClick={async (e) => {
                    e.preventDefault()
                    setPendingFollow(user.username)
                    try {
                      await followUser(user.username)
                      toast({ title: `Following ${user.username}` })
                      setSuggestions((prev) => prev.filter((u) => u.username !== user.username))
                    } catch (e) {
                      toast({
                        title: "Follow failed",
                        description: e instanceof Error ? e.message : "Please try again.",
                        variant: "destructive",
                      })
                    } finally {
                      setPendingFollow(null)
                    }
                  }}
                  className="hover:bg-primary/10 hover:text-primary hover:border-primary bg-transparent"
                >
                  {pendingFollow === user.username ? "Following..." : "Follow"}
                </Button>
              )
            )}
          </div>
        )}
      </Card>
    </div>
  )
}
