import { useState, useEffect } from "react"
import { Card } from "@/components/ui/card"
import { Users, GraduationCap, BookOpen, TrendingUp } from "lucide-react"
import { getSchoolClient } from "@/lib/onvm-client"

const PROGRAM_ID = "918c95aa44b129ae2315698637afbb339b7cb43ed6115e433605ae172f617bc4"

export function DashboardView() {
  const [stats, setStats] = useState([
    {
      title: "Total Students",
      value: "0",
      change: "+0%",
      icon: Users,
      color: "text-blue-500",
      bgColor: "bg-blue-500/10",
    },
    {
      title: "Total Teachers",
      value: "0",
      change: "+0%",
      icon: GraduationCap,
      color: "text-emerald-500",
      bgColor: "bg-emerald-500/10",
    },
    {
      title: "Active Courses",
      value: "0",
      change: "+0%",
      icon: BookOpen,
      color: "text-purple-500",
      bgColor: "bg-purple-500/10",
    },
    {
      title: "Average Grade",
      value: "0%",
      change: "+0%",
      icon: TrendingUp,
      color: "text-amber-500",
      bgColor: "bg-amber-500/10",
    },
  ])
  
  const client = getSchoolClient()
  
  useEffect(() => {
    fetchStats()
  }, [])
  
  const fetchStats = async () => {
    try {
      // Fetch students count
      const studentsResponse = await client.listStudents()
      const studentsCount = studentsResponse.status === "success" && studentsResponse.data ? studentsResponse.data.length : 0
      
      // Fetch teachers count
      const teachersResponse = await client.listTeachers()
      const teachersCount = teachersResponse.status === "success" && teachersResponse.data ? teachersResponse.data.length : 0
      
      // Fetch courses count
      const coursesResponse = await client.listCourses()
      const coursesCount = coursesResponse.status === "success" && coursesResponse.data ? coursesResponse.data.length : 0
      
      // Update stats with real data
      setStats([
        {
          title: "Total Students",
          value: studentsCount.toString(),
          change: "+0%", // In a real app, you would calculate this
          icon: Users,
          color: "text-blue-500",
          bgColor: "bg-blue-500/10",
        },
        {
          title: "Total Teachers",
          value: teachersCount.toString(),
          change: "+0%",
          icon: GraduationCap,
          color: "text-emerald-500",
          bgColor: "bg-emerald-500/10",
        },
        {
          title: "Active Courses",
          value: coursesCount.toString(),
          change: "+0%",
          icon: BookOpen,
          color: "text-purple-500",
          bgColor: "bg-purple-500/10",
        },
        {
          title: "Average Grade",
          value: "85.3%", // In a real app, you would calculate this
          change: "+0%",
          icon: TrendingUp,
          color: "text-amber-500",
          bgColor: "bg-amber-500/10",
        },
      ])
    } catch (error) {
      console.error("Failed to fetch stats:", error)
    }
  }

  const recentActivity = [
    { student: "Emma Wilson", action: "Enrolled in Mathematics 101", time: "2 hours ago" },
    { student: "James Brown", action: "Received grade A in Physics", time: "4 hours ago" },
    { student: "Sophia Davis", action: "Completed Chemistry assignment", time: "5 hours ago" },
    { student: "Oliver Smith", action: "Enrolled in Literature 201", time: "6 hours ago" },
  ]

  return (
    <div className="space-y-6">
      {/* Stats Grid */}
      <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-4">
        {stats.map((stat) => {
          const Icon = stat.icon
          return (
            <Card key={stat.title} className="p-6">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-sm font-medium text-muted-foreground">{stat.title}</p>
                  <p className="mt-2 text-3xl font-bold text-foreground">{stat.value}</p>
                  <p className="mt-1 text-sm text-emerald-600 dark:text-emerald-400">{stat.change} from last month</p>
                </div>
                <div className={`flex size-12 items-center justify-center rounded-xl ${stat.bgColor}`}>
                  <Icon className={`size-6 ${stat.color}`} />
                </div>
              </div>
            </Card>
          )
        })}
      </div>

      {/* Recent Activity */}
      <div className="grid gap-6 lg:grid-cols-2">
        <Card className="p-6">
          <h3 className="text-lg font-semibold text-foreground mb-4">Recent Activity</h3>
          <div className="space-y-4">
            {recentActivity.map((activity, i) => (
              <div key={i} className="flex items-start gap-3 pb-4 border-b border-border last:border-0 last:pb-0">
                <div className="flex size-10 items-center justify-center rounded-full bg-primary/10 text-primary font-semibold text-sm flex-shrink-0">
                  {activity.student
                    .split(" ")
                    .map((n) => n[0])
                    .join("")}
                </div>
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium text-foreground">{activity.student}</p>
                  <p className="text-sm text-muted-foreground">{activity.action}</p>
                  <p className="text-xs text-muted-foreground mt-1">{activity.time}</p>
                </div>
              </div>
            ))}
          </div>
        </Card>

        <Card className="p-6">
          <h3 className="text-lg font-semibold text-foreground mb-4">Quick Stats</h3>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-sm text-muted-foreground">Students enrolled this week</span>
              <span className="text-lg font-semibold text-foreground">23</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm text-muted-foreground">Assignments graded</span>
              <span className="text-lg font-semibold text-foreground">156</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm text-muted-foreground">Course completion rate</span>
              <span className="text-lg font-semibold text-foreground">92%</span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm text-muted-foreground">Average attendance</span>
              <span className="text-lg font-semibold text-foreground">94%</span>
            </div>
          </div>
        </Card>
      </div>
    </div>
  )
}