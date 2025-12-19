"use client"

import { useRef, useState } from "react"
import { Avatar, AvatarFallback } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { ImageIcon, Smile, Send, X } from "lucide-react"
import { createPost, uploadBlob } from "@/lib/api"
import { getCurrentUsername } from "@/lib/auth"
import { useToast } from "@/hooks/use-toast"

interface CreatePostBoxProps {
  onPostCreated?: () => void
}

export function CreatePostBox({ onPostCreated }: CreatePostBoxProps) {
  const [content, setContent] = useState("")
  const [isFocused, setIsFocused] = useState(false)
  const [isPosting, setIsPosting] = useState(false)
  const [attachments, setAttachments] = useState<{ id: string; name: string; size: number }[]>([])
  const fileInputRef = useRef<HTMLInputElement | null>(null)
  const { toast } = useToast()
  const username = getCurrentUsername()

  const handlePost = async () => {
    if (!content.trim()) return

    setIsPosting(true)
    try {
      const attachmentIds = attachments.map((a) => a.id)
      await createPost(content.trim(), attachmentIds)

      toast({
        title: "Posted!",
        description: "Your post has been shared.",
      })

      setContent("")
      setIsFocused(false)
      setAttachments([])

      if (onPostCreated) {
        onPostCreated()
      }
    } catch (error) {
      console.error("[v0] Post creation error:", error)
      toast({
        title: "Failed to post",
        description: error instanceof Error ? error.message : "Please try again.",
        variant: "destructive",
      })
    } finally {
      setIsPosting(false)
    }
  }

  const getInitials = (name: string | null) => {
    if (!name) return "U"
    return name.substring(0, 2).toUpperCase()
  }

  const handleFileSelect = async (files: FileList | null) => {
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
    <div className="p-4">
      <div className="flex gap-3">
        <Avatar className="w-10 h-10 border-2 border-primary/20 shrink-0">
          <AvatarFallback className="bg-gradient-to-br from-primary to-accent text-primary-foreground">
            {getInitials(username)}
          </AvatarFallback>
        </Avatar>

        <div className="flex-1">
          <Textarea
            placeholder="What's on your mind?"
            value={content}
            onChange={(e) => setContent(e.target.value)}
            onFocus={() => setIsFocused(true)}
            className="min-h-[80px] resize-none bg-transparent border-none focus-visible:ring-0 text-base placeholder:text-muted-foreground"
          />

          {attachments.length > 0 && (
            <div className="mt-3 space-y-2">
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

          {(isFocused || content) && (
            <div className="flex items-center justify-between mt-3 pt-3 border-t border-border/50 animate-in slide-in-from-bottom-2 duration-200">
              <div className="flex items-center gap-2">
                <input
                  ref={fileInputRef}
                  type="file"
                  multiple
                  className="hidden"
                  onChange={(e) => handleFileSelect(e.target.files)}
                />
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => fileInputRef.current?.click()}
                  className="text-primary hover:bg-primary/10 hover:text-primary"
                >
                  <ImageIcon className="w-4 h-4" />
                </Button>
                <Button size="sm" variant="ghost" className="text-primary hover:bg-primary/10 hover:text-primary">
                  <Smile className="w-4 h-4" />
                </Button>
              </div>

              <div className="flex items-center gap-2">
                <span className="text-xs text-muted-foreground">{content.length}/2048</span>
                <Button
                  size="sm"
                  onClick={handlePost}
                  disabled={!content.trim() || isPosting}
                  className="bg-gradient-to-r from-primary to-accent hover:opacity-90 transition-all hover:scale-105"
                >
                  <Send className="w-4 h-4 mr-1" />
                  {isPosting ? "Posting..." : "Post"}
                </Button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
