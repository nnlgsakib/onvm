import { LandingHero } from "@/components/landing/landing-hero"
import { LandingFeatures } from "@/components/landing/landing-features"
import { LandingStats } from "@/components/landing/landing-stats"
import { LandingCTA } from "@/components/landing/landing-cta"
import { LandingNav } from "@/components/landing/landing-nav"

export default function HomePage() {
  return (
    <div className="min-h-screen bg-background">
      <LandingNav />
      <LandingHero />
      <LandingStats />
      <LandingFeatures />
      <LandingCTA />
    </div>
  )
}
