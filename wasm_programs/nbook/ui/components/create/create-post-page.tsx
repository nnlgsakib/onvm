"use client"

import { useState } from "react"
import { useRouter } from "next/navigation"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { Card } from "@/components/ui/card"
import { Loader2, Send, ImageIcon, X } from "lucide-react"
import { useToast } from "@/hooks/use-toast"
import { createPost, uploadBlob } from "@/lib/api"
import { getCurrentUsername, getSession } from "@/lib/auth"

export function CreatePostPage() {
  const [content, setContent] = useState("")
  const [isPosting, setIsPosting] = useState(false)
  const [attachments, setAttachments] = useState<{ id: string; name: string; size: number }[]>([])
  const router = useRouter()
  const { toast } = useToast()
  const session = getSession()
  const username = getCurrentUsername()

  const handlePost = async () => {
    if (!content.trim()) {
      toast({
        title: "Post is empty",
        description: "Please write something before posting.",
        variant: "destructive",
      })
      return
    }

    if (!session?.session) {
      toast({
        title: "Login required",
        description: "Please log in before creating a post.",
        variant: "destructive",
      })
      return
    }

    setIsPosting(true)

    try {
      const attIds = attachments.map((a) => a.id)
      await createPost(content.trim(), attIds)

      toast({
        title: "Post published!",
        description: "Your post has been shared.",
      })

      router.push("/feed")
    } catch (error) {
      toast({
        title: "Failed to post",
        description: error instanceof Error ? error.message : "Please try again.",
        variant: "destructive",
      })
    } finally {
      setIsPosting(false)
    }
  }

  const handleFiles = async (files: FileList | null) => {
    if (!files || files.length === 0) return
    setIsPosting(true)
    try {
      const uploads = []
      for (const file of Array.from(files)) {
        const blobId = await uploadBlob(file)
        uploads.push({ id: blobId, name: file.name, size: file.size })
      }
      setAttachments((prev) => [...prev, ...uploads])
    } catch (e) {
      toast({
        title: "Upload failed",
        description: e instanceof Error ? e.message : "Please try again.",
        variant: "destructive",
      })
    } finally {
      setIsPosting(false)
    }
  }

  return (
    <div className="max-w-2xl mx-auto">
      {/* Header */}
      <div className="sticky top-0 z-40 bg-background/80 backdrop-blur-xl border-b border-border/50">
        <div className="flex items-center justify-between p-4">
          <h1 className="text-2xl font-bold">Create Post</h1>
          <Button
            onClick={handlePost}
            disabled={isPosting || !content.trim()}
            className="bg-gradient-to-r from-primary to-accent hover:opacity-90 transition-all hover:scale-105"
          >
            {isPosting ? (
              <>
                <Loader2 className="w-4 h-4 mr-2 animate-spin" />
                Posting...
              </>
            ) : (
              <>
                <Send className="w-4 h-4 mr-2" />
                Publish
              </>
            )}
          </Button>
        </div>
      </div>

      {/* Create Post Form */}
      <Card className="border-x-0 border-t-0 border-b border-border/50 rounded-none bg-transparent p-4">
        <div className="flex gap-3">
          <Avatar className="w-12 h-12 border-2 border-primary/20">
            <AvatarFallback className="bg-gradient-to-br from-primary to-accent text-primary-foreground">
              {(username || "NB").substring(0, 2).toUpperCase()}
            </AvatarFallback>
          </Avatar>

          <div className="flex-1">
            <Textarea
              placeholder="What's happening?"
              value={content}
              onChange={(e) => setContent(e.target.value)}
              className="min-h-[200px] text-lg resize-none bg-transparent border-none focus-visible:ring-0 placeholder:text-muted-foreground"
            />

            {attachments.length > 0 && (
              <div className="mt-4 space-y-2">
                {attachments.map((att, idx) => (
                  <div
                    key={att.id}
                    className="flex items-center justify-between rounded-md border border-border/60 px-3 py-2 text-sm"
                  >
                    <div className="truncate">
                      {att.name} ({Math.round(att.size / 1024)} KB)
                    </div>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => setAttachments((prev) => prev.filter((_, i) => i !== idx))}
                      className="h-8 w-8"
                    >
                      <X className="w-4 h-4" />
                    </Button>
                  </div>
                ))}
              </div>
            )}

            {/* Actions */}
            <div className="flex items-center justify-between mt-4 pt-4 border-t border-border/50">
              <div className="flex items-center gap-2">
                <label className="flex items-center gap-2 text-sm text-primary hover:cursor-pointer">
                  <ImageIcon className="w-4 h-4" />
                  <span>Add attachments</span>
                  <input
                    type="file"
                    multiple
                    className="hidden"
                    onChange={(e) => handleFiles(e.target.files)}
                  />
                </label>
              </div>

              <div className="flex items-center gap-3">
                <span
                  className={`text-sm ${
                    content.length > 2048 ? "text-destructive" : "text-muted-foreground"
                  } transition-colors`}
                >
                  {content.length}/2048
                </span>
              </div>
            </div>
          </div>
        </div>
      </Card>

      {/* Tips */}
      <Card className="m-4 p-4 bg-card/50 border-border/50 backdrop-blur-sm">
        <h3 className="font-semibold mb-2 text-sm">Tips for great posts:</h3>
        <ul className="text-sm text-muted-foreground space-y-1">
          <li>• Keep it concise and engaging</li>
          <li>• Use hashtags to reach more people</li>
          <li>• Add images or media for better engagement</li>
          <li>• Be authentic and share your unique perspective</li>
        </ul>
      </Card>
    </div>
  )
}
