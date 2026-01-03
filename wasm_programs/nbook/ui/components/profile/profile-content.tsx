"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import { useSearchParams } from "next/navigation"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { PostCard } from "@/components/feed/post-card"
import { Calendar, Settings, Image as ImageIcon } from "lucide-react"
import { useToast } from "@/hooks/use-toast"
import {
  getProfile,
  getPost,
  updateProfile,
  uploadBlob,
  listFollowers,
  listFollowing,
  followUser,
  unfollowUser,
} from "@/lib/api"
import { getSession } from "@/lib/auth"
import type { Post, ProfileView, UserSummary } from "@/lib/onvm-client"

export function ProfileContent() {
  const searchParams = useSearchParams()
  const requestedUser = searchParams.get("user")
  const [profile, setProfile] = useState<ProfileView | null>(null)
  const [posts, setPosts] = useState<Post[]>([])
  const [loading, setLoading] = useState(true)
  const [editing, setEditing] = useState(false)
  const [displayName, setDisplayName] = useState("")
  const [bio, setBio] = useState("")
  const [avatarBlobId, setAvatarBlobId] = useState<string | undefined>(undefined)
  const [bannerBlobId, setBannerBlobId] = useState<string | undefined>(undefined)
  const avatarInputRef = useRef<HTMLInputElement | null>(null)
  const bannerInputRef = useRef<HTMLInputElement | null>(null)
  const { toast } = useToast()
  const [sessionUser, setSessionUser] = useState<string | null>(null)
  const [hydrated, setHydrated] = useState(false)
  const [followerModalOpen, setFollowerModalOpen] = useState<"followers" | "following" | null>(null)
  const [followerList, setFollowerList] = useState<UserSummary[]>([])
  const [followingList, setFollowingList] = useState<UserSummary[]>([])
  const [listLoading, setListLoading] = useState(false)
  const [isFollowing, setIsFollowing] = useState(false)
  const [followBusy, setFollowBusy] = useState(false)

  useEffect(() => {
    const s = getSession()
    setSessionUser(s?.username ?? null)
    setHydrated(true)
  }, [])

  useEffect(() => {
    const load = async () => {
      if (!hydrated) return
      const targetUser = requestedUser || sessionUser
      if (!targetUser) {
        setLoading(false)
        return
      }
      try {
        const p = await getProfile(targetUser)
        setProfile(p)
        setDisplayName(p.displayName || "")
        setBio(p.bio || "")
        setAvatarBlobId(p.avatarBlobId || undefined)
        setBannerBlobId(p.bannerBlobId || undefined)
        // Initially hydrate lists as bare usernames; full summaries are loaded on demand.
        setFollowerList((p.followerList || []).map((u) => ({ username: u, followers: 0 } as UserSummary)))
        setFollowingList((p.followingList || []).map((u) => ({ username: u, followers: 0 } as UserSummary)))
        setIsFollowing(false)
        if (sessionUser && (p.followerList || []).includes(sessionUser)) {
          setIsFollowing(true)
        }

        // Load posts authored by this user
        const fetched = await Promise.all(
          (p.posts || []).map(async (id) => {
            try {
              return await getPost(id)
            } catch {
              return null
            }
          })
        )
        setPosts(fetched.filter((p): p is Post => Boolean(p)))
      } catch (error) {
        toast({
          title: "Failed to load profile",
          description: error instanceof Error ? error.message : "Please try again.",
          variant: "destructive",
        })
      } finally {
        setLoading(false)
      }
    }

    load()
  }, [sessionUser, requestedUser, toast, hydrated])

  const joinedText = useMemo(() => {
    if (!profile) return ""
    const date = new Date(profile.createdAt)
    return `Joined ${date.toLocaleString(undefined, { month: "long", year: "numeric" })}`
  }, [profile])

  const isSelf = !requestedUser || requestedUser === sessionUser

  const toggleFollow = async () => {
    if (!sessionUser || !profile) {
      toast({
        title: "Please log in",
        description: "You need to be signed in to follow people.",
      })
      return
    }
    setFollowBusy(true)
    try {
      if (isFollowing) {
        await unfollowUser(profile.username)
        setIsFollowing(false)
        setProfile((p) =>
          p
            ? {
                ...p,
                followers: Math.max(0, p.followers - 1),
                followerList: p.followerList.filter((u) => u !== sessionUser),
              }
            : p
        )
        toast({ title: `Unfollowed ${profile.username}` })
      } else {
        await followUser(profile.username)
        setIsFollowing(true)
        setProfile((p) =>
          p
            ? {
                ...p,
                followers: p.followers + 1,
                followerList: p.followerList.includes(sessionUser)
                  ? p.followerList
                  : [...p.followerList, sessionUser],
              }
            : p
        )
        toast({ title: `Following ${profile.username}` })
      }
    } catch (e) {
      toast({
        title: "Follow action failed",
        description: e instanceof Error ? e.message : "Please try again.",
        variant: "destructive",
      })
    } finally {
      setFollowBusy(false)
    }
  }

  if (!hydrated || loading) {
    return (
      <div className="max-w-2xl mx-auto p-6 text-center text-muted-foreground">
        Loading profile...
      </div>
    )
  }

  if (!sessionUser && !requestedUser) {
    return (
      <div className="max-w-2xl mx-auto p-6 text-center text-muted-foreground">
        Log in to view your profile.
      </div>
    )
  }

  if (!profile) {
    return (
      <div className="max-w-2xl mx-auto p-6 text-center text-muted-foreground">
        Profile not found.
      </div>
    )
  }

  const initials = (profile.username || "NB").substring(0, 2).toUpperCase()

  return (
    <div className="w-full max-w-2xl mx-auto px-2 sm:px-0">
      <div className="sticky top-0 z-40 bg-background/80 backdrop-blur-xl border-b border-border/50">
        <div className="p-4">
          <div className="flex items-center justify-between">
            <div>
              <h1 className="text-xl font-bold">{profile.displayName || profile.username}</h1>
              <p className="text-sm text-muted-foreground">{posts.length} posts</p>
            </div>
            <div className="flex items-center gap-2">
              {!isSelf ? (
                <Button
                  size="sm"
                  variant={isFollowing ? "outline" : "default"}
                  onClick={toggleFollow}
                  disabled={followBusy}
                  className="min-w-[96px]"
                >
                  {followBusy ? "..." : isFollowing ? "Unfollow" : "Follow"}
                </Button>
              ) : null}
              <Button size="sm" variant="ghost">
                <Settings className="w-4 h-4" />
              </Button>
            </div>
          </div>
        </div>
      </div>

      <div className="relative h-48 bg-gradient-to-br from-primary via-accent to-primary overflow-hidden">
        {profile.bannerBlobId ? (
          <img
            src={`/api/onvm/blob/${profile.bannerBlobId}`}
            alt="banner"
            className="absolute inset-0 h-full w-full object-cover"
          />
        ) : (
          <div className="absolute inset-0 bg-[radial-gradient(ellipse_at_center,_var(--tw-gradient-stops))] from-transparent via-background/20 to-background/40" />
        )}
      </div>

      <div className="px-4">
        <div className="flex items-end justify-between -mt-16 mb-4">
          <Avatar className="w-32 h-32 border-4 border-background shadow-2xl">
            {profile.avatarBlobId ? (
              <img
                src={`/api/onvm/blob/${profile.avatarBlobId}`}
                alt="avatar"
                className="w-full h-full object-cover rounded-full"
              />
            ) : (
              <AvatarFallback className="bg-gradient-to-br from-primary to-accent text-primary-foreground text-3xl">
                {initials}
              </AvatarFallback>
            )}
          </Avatar>
          {isSelf && (
            <Button
              variant="outline"
              onClick={() => setEditing((v) => !v)}
              className="hover:bg-primary/10 hover:text-primary hover:border-primary transition-all hover:scale-105 bg-transparent"
            >
              {editing ? "Cancel" : "Edit Profile"}
            </Button>
          )}
        </div>

        <div className="space-y-3">
          <div>
            <h2 className="text-2xl font-bold">{profile.displayName || profile.username}</h2>
            <p className="text-muted-foreground mt-2 leading-relaxed">
              {profile.bio || "Secure-by-default profile powered by ONVM."}
            </p>
          </div>

          <div className="flex flex-wrap items-center gap-4 text-sm text-muted-foreground">
            <div className="flex items-center gap-1">
              <Calendar className="w-4 h-4" />
              <span>{joinedText}</span>
            </div>
          </div>

          <div className="flex items-center gap-6 pt-2">
            <button
              className="text-left"
              onClick={async () => {
                if (!sessionUser) return
                setListLoading(true)
                try {
                  const users = await listFollowing(profile.username)
                  setFollowingList(users)
                  setFollowerModalOpen("following")
                } catch (e) {
                  toast({
                    title: "Failed to load following",
                    description: e instanceof Error ? e.message : "Please try again.",
                    variant: "destructive",
                  })
                } finally {
                  setListLoading(false)
                }
              }}
            >
              <span className="font-bold">{profile.following}</span>{" "}
              <span className="text-muted-foreground">Following</span>
            </button>
            <button
              className="text-left"
              onClick={async () => {
                if (!sessionUser) return
                setListLoading(true)
                try {
                  const users = await listFollowers(profile.username)
                  setFollowerList(users)
                  setFollowerModalOpen("followers")
                } catch (e) {
                  toast({
                    title: "Failed to load followers",
                    description: e instanceof Error ? e.message : "Please try again.",
                    variant: "destructive",
                  })
                } finally {
                  setListLoading(false)
                }
              }}
            >
              <span className="font-bold">{profile.followers}</span>{" "}
              <span className="text-muted-foreground">Followers</span>
            </button>
          </div>
        </div>

        {isSelf && editing && (
          <div className="mt-4 space-y-3">
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Display name</label>
              <input
                className="w-full rounded-md border border-border/60 bg-background px-3 py-2 text-sm"
                value={displayName}
                onChange={(e) => setDisplayName(e.target.value)}
                maxLength={64}
              />
            </div>
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Bio</label>
              <textarea
                className="w-full rounded-md border border-border/60 bg-background px-3 py-2 text-sm min-h-[80px]"
                value={bio}
                onChange={(e) => setBio(e.target.value)}
                maxLength={280}
              />
            </div>
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Avatar (blob&lt;hash&gt;, optional)</label>
              <input
                className="w-full rounded-md border border-border/60 bg-background px-3 py-2 text-sm"
                value={avatarBlobId || ""}
                onChange={(e) => setAvatarBlobId(e.target.value || undefined)}
              />
              <input
                ref={avatarInputRef}
                type="file"
                accept="image/*"
                className="hidden"
                onChange={async (e) => {
                  const file = e.target.files?.[0]
                  if (!file) return
                  try {
                    const id = await uploadBlob(file)
                    setAvatarBlobId(id)
                    toast({ title: "Avatar uploaded" })
                  } catch (err) {
                    toast({
                      title: "Upload failed",
                      description: err instanceof Error ? err.message : "Please try again.",
                      variant: "destructive",
                    })
                  }
                }}
              />
              <Button
                size="sm"
                variant="outline"
                className="mt-1 flex items-center gap-2"
                onClick={() => avatarInputRef.current?.click()}
              >
                <ImageIcon className="w-4 h-4" />
                Upload avatar
              </Button>
            </div>
            <div className="space-y-1">
              <label className="text-sm text-muted-foreground">Banner (blob&lt;hash&gt;, optional)</label>
              <input
                className="w-full rounded-md border border-border/60 bg-background px-3 py-2 text-sm"
                value={bannerBlobId || ""}
                onChange={(e) => setBannerBlobId(e.target.value || undefined)}
              />
              <input
                ref={bannerInputRef}
                type="file"
                accept="image/*,video/*"
                className="hidden"
                onChange={async (e) => {
                  const file = e.target.files?.[0]
                  if (!file) return
                  try {
                    const id = await uploadBlob(file)
                    setBannerBlobId(id)
                    toast({ title: "Banner uploaded" })
                  } catch (err) {
                    toast({
                      title: "Upload failed",
                      description: err instanceof Error ? err.message : "Please try again.",
                      variant: "destructive",
                    })
                  }
                }}
              />
              <Button
                size="sm"
                variant="outline"
                className="mt-1 flex items-center gap-2"
                onClick={() => bannerInputRef.current?.click()}
              >
                <ImageIcon className="w-4 h-4" />
                Upload banner
              </Button>
            </div>
            <Button
              size="sm"
              variant="outline"
              onClick={async () => {
                try {
                  await updateProfile(
                    displayName || undefined,
                    bio || undefined,
                    avatarBlobId || undefined,
                    bannerBlobId || undefined
                  )
                  toast({ title: "Profile updated" })
                  setEditing(false)
                  const updated = await getProfile(profile.username)
                  setProfile(updated)
                } catch (e) {
                  toast({
                    title: "Update failed",
                    description: e instanceof Error ? e.message : "Please try again.",
                    variant: "destructive",
                  })
                }
              }}
            >
              Save changes
            </Button>
          </div>
        )}

        <div className="flex border-b border-border/50 mt-6 -mx-4 px-4">
          <button className="flex-1 py-4 text-sm font-medium transition-colors relative text-foreground">
            Posts
            <div className="absolute bottom-0 left-0 right-0 h-1 bg-gradient-to-r from-primary to-accent rounded-t-full" />
          </button>
        </div>
      </div>

      <div className="mt-0">
        {posts.length === 0 ? (
          <div className="p-8 text-center text-muted-foreground">No posts yet.</div>
        ) : (
          posts.map((post) => <PostCard key={post.id} post={post} />)
        )}
      </div>

      {followerModalOpen && (
        <div className="fixed inset-0 z-50 bg-black/50 backdrop-blur-sm flex items-center justify-center px-4">
          <div className="bg-background rounded-lg shadow-lg max-w-md w-full p-4 space-y-3">
            <div className="flex items-center justify-between">
              <h3 className="text-lg font-semibold">
                {followerModalOpen === "followers" ? "Followers" : "Following"}
              </h3>
              <Button variant="ghost" size="sm" onClick={() => setFollowerModalOpen(null)}>
                Close
              </Button>
            </div>
            {listLoading ? (
              <div className="text-sm text-muted-foreground">Loading...</div>
            ) : (
              <div className="space-y-2 max-h-80 overflow-y-auto">
                {(followerModalOpen === "followers" ? followerList : followingList).map((u) => (
                  <div key={u.username} className="flex items-center justify-between rounded-md border border-border/50 px-3 py-2 gap-3">
                    <div className="flex items-center gap-2">
                      <Avatar className="w-8 h-8">
                        {u.avatarBlobId ? (
                          <img
                            src={`/api/onvm/blob/${u.avatarBlobId}`}
                            alt={u.username}
                            className="w-full h-full object-cover rounded-full"
                            onError={(e) => {
                              const target = e.currentTarget
                              target.style.display = "none"
                            }}
                          />
                        ) : null}
                        <AvatarFallback className="bg-gradient-to-br from-primary/20 to-accent/20">
                          {(u.displayName || u.username || "NB").substring(0, 2).toUpperCase()}
                        </AvatarFallback>
                      </Avatar>
                      <a href={`/profile?user=${encodeURIComponent(u.username)}`} className="text-sm font-medium hover:text-primary">
                        {u.displayName || u.username}
                      </a>
                    </div>
                    {followerModalOpen === "followers" && sessionUser === profile.username ? (
                      <Button
                        size="sm"
                        variant="outline"
                        onClick={async () => {
                          try {
                            await followUser(u.username)
                            toast({ title: `Followed back ${u.username}` })
                          } catch (e) {
                            toast({
                              title: "Follow back failed",
                              description: e instanceof Error ? e.message : "Please try again.",
                              variant: "destructive",
                            })
                          }
                        }}
                      >
                        Follow back
                      </Button>
                    ) : null}
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
