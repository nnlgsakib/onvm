"use client"

import Link from "next/link"
import { usePathname } from "next/navigation"
import { useEffect, useState } from "react"
import { Button } from "@/components/ui/button"
import { Home, Compass, User, Settings, LogOut, Plus } from "lucide-react"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { getSession, logout } from "@/lib/auth"
import { getProfile } from "@/lib/api"

export function Sidebar() {
  const pathname = usePathname()
  const [username, setUsername] = useState<string | null>(null)
  const [hasSession, setHasSession] = useState(false)
  const [avatarId, setAvatarId] = useState<string | null>(null)

  useEffect(() => {
    const session = getSession()
    setUsername(session?.username ?? null)
    setHasSession(Boolean(session))
    if (session?.username) {
      getProfile(session.username)
        .then((p) => setAvatarId(p.avatarBlobId || null))
        .catch(() => setAvatarId(null))
    }
  }, [])

  const navItems = [
    { icon: Home, label: "Home", href: "/feed", active: pathname === "/feed" },
    { icon: Compass, label: "Discover", href: "/discover", active: pathname === "/discover" },
    { icon: Plus, label: "Create", href: "/create", active: pathname === "/create" },
    { icon: User, label: "Profile", href: "/profile", active: pathname === "/profile" },
    { icon: Settings, label: "Settings", href: "/settings", active: pathname === "/settings" },
  ]

  return (
    <div className="flex flex-col h-full p-4 space-y-4">
      {/* Logo */}
      <Link href="/feed" className="flex items-center gap-3 p-2 group rounded-xl hover:bg-secondary/40 transition">
        <div className="relative">
          <div className="absolute inset-0 bg-primary/30 blur-lg group-hover:bg-primary/50 transition-all" />
          <div className="relative flex items-center justify-center w-10 h-10 rounded-xl bg-gradient-to-br from-primary to-accent">
            <svg className="w-6 h-6 text-primary-foreground" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253"
              />
            </svg>
          </div>
        </div>
        <span className="text-2xl font-bold bg-gradient-to-r from-primary to-accent bg-clip-text text-transparent">
          nbook
        </span>
      </Link>

      {/* Navigation */}
      <nav className="flex-1 space-y-2">
        {navItems.map((item) => {
          const Icon = item.icon
          return (
            <Link key={item.href} href={item.href}>
              <Button
                variant="ghost"
                className={`w-full justify-start gap-3 h-11 rounded-xl transition-all duration-200 ${
                  item.active
                    ? "bg-primary/10 text-primary hover:bg-primary/20"
                    : "hover:bg-secondary/50 text-muted-foreground hover:text-foreground"
                }`}
              >
                <Icon className={`w-5 h-5 ${item.active ? "scale-110" : ""} transition-transform`} />
                <span className="text-base">{item.label}</span>
              </Button>
            </Link>
          )
        })}
      </nav>

      {/* User Profile */}
      <div className="pt-4 border-t border-border/50 mt-auto">
        <Link href={hasSession ? "/profile" : "/login"} className="flex items-center gap-3 p-2 rounded-xl hover:bg-secondary/50 transition-colors group">
          <Avatar className="w-10 h-10 border-2 border-primary/20 group-hover:border-primary/40 transition-colors">
            {avatarId ? (
              <img
                src={`/api/onvm/blob/${avatarId}`}
                alt={username || "avatar"}
                className="w-full h-full object-cover rounded-full"
              />
            ) : (
              <AvatarFallback className="bg-gradient-to-br from-primary to-accent text-primary-foreground">
                {(username || "NB").substring(0, 2).toUpperCase()}
              </AvatarFallback>
            )}
          </Avatar>
          <div className="flex-1 min-w-0">
            <div className="text-sm font-medium truncate">{username || "Guest"}</div>
            <div className="text-xs text-muted-foreground">{hasSession ? "View profile" : "Log in"}</div>
          </div>
        </Link>

        {hasSession && (
          <Button
            variant="ghost"
            className="w-full justify-start gap-3 mt-2 text-muted-foreground hover:text-destructive hover:bg-destructive/10"
            onClick={() => {
              logout()
              setUsername(null)
              setHasSession(false)
            }}
          >
            <LogOut className="w-5 h-5" />
            <span>Logout</span>
          </Button>
        )}
      </div>
    </div>
  )
}
