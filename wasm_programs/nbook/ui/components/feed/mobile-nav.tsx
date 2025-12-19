"use client"

import Link from "next/link"
import { usePathname } from "next/navigation"
import { Home, Compass, User, Plus } from "lucide-react"

export function MobileNav() {
  const pathname = usePathname()

  const navItems = [
    { icon: Home, label: "Home", href: "/feed" },
    { icon: Compass, label: "Discover", href: "/discover" },
    { icon: Plus, label: "Post", href: "/create" },
    { icon: User, label: "Profile", href: "/profile" },
  ]

  return (
    <nav className="fixed bottom-0 left-0 right-0 h-16 bg-card/90 backdrop-blur-xl border-t border-border/50 z-50">
      <div className="grid grid-cols-5 items-center h-full px-2">
        {navItems.map((item) => {
          const Icon = item.icon
          const isActive = pathname === item.href
          const isCreate = item.href === "/create"

          if (isCreate) {
            return (
              <div key={item.href} className="col-span-1 col-start-3 flex justify-center">
                <Link
                  href={item.href}
                  className="flex flex-col items-center justify-center w-14 h-14 rounded-full bg-gradient-to-br from-primary to-accent text-primary-foreground hover:scale-110 transition-transform shadow-lg shadow-primary/30"
                >
                  <Icon className="w-6 h-6" />
                </Link>
              </div>
            )
          }

          return (
            <Link
              key={item.href}
              href={item.href}
              className={`flex flex-col items-center justify-center gap-1 px-3 py-2 rounded-lg transition-all ${
                isActive ? "text-primary" : "text-muted-foreground"
              }`}
            >
              <Icon className={`w-5 h-5 ${isActive ? "scale-110" : ""} transition-transform`} />
              <span className="text-xs">{item.label}</span>
            </Link>
          )
        })}
      </div>
    </nav>
  )
}
