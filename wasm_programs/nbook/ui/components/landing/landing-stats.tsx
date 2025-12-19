"use client"

import type React from "react"

import { useEffect, useState, useRef } from "react"
import { TrendingUp, Users, MessageCircle, Heart } from "lucide-react"

export function LandingStats() {
  return (
    <section className="py-20 border-y border-border/50 bg-card/30 backdrop-blur-sm">
      <div className="container mx-auto px-4">
        <div className="grid grid-cols-2 md:grid-cols-4 gap-8">
          <StatCard icon={<Users className="w-6 h-6" />} value="100K+" label="Active Users" delay={0} />
          <StatCard icon={<MessageCircle className="w-6 h-6" />} value="5M+" label="Posts Shared" delay={100} />
          <StatCard icon={<Heart className="w-6 h-6" />} value="50M+" label="Interactions" delay={200} />
          <StatCard icon={<TrendingUp className="w-6 h-6" />} value="99.9%" label="Uptime" delay={300} />
        </div>
      </div>
    </section>
  )
}

function StatCard({
  icon,
  value,
  label,
  delay,
}: {
  icon: React.ReactNode
  value: string
  label: string
  delay: number
}) {
  const [isVisible, setIsVisible] = useState(false)
  const [count, setCount] = useState(0)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setIsVisible(true)
        }
      },
      { threshold: 0.1 },
    )

    if (ref.current) {
      observer.observe(ref.current)
    }

    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    if (isVisible) {
      const timer = setTimeout(() => {
        setCount(1)
      }, delay)
      return () => clearTimeout(timer)
    }
  }, [isVisible, delay])

  return (
    <div
      ref={ref}
      className={`text-center transition-all duration-700 ${
        count > 0 ? "opacity-100 translate-y-0" : "opacity-0 translate-y-8"
      }`}
    >
      <div className="inline-flex items-center justify-center w-12 h-12 rounded-xl bg-gradient-to-br from-primary/20 to-accent/20 text-primary mb-3 hover:scale-110 transition-transform duration-300">
        {icon}
      </div>
      <div className="text-3xl md:text-4xl font-bold mb-1 bg-gradient-to-r from-primary to-accent bg-clip-text text-transparent">
        {value}
      </div>
      <div className="text-sm text-muted-foreground">{label}</div>
    </div>
  )
}
