import { NextRequest, NextResponse } from 'next/server'
import { OnvmClient } from 'onvm-sdk'

const RPC_ENDPOINT = process.env.ONVM_RPC_ENDPOINT
const PROGRAM_ID = process.env.ONVM_PROGRAM_ID 
const PROJECT_ID = process.env.ONVM_PROJECT_ID 
const PROJECT_SECRET = process.env.PROJECT_SECRET 
let clientInstance: OnvmClient | null = null

function getClient(): OnvmClient {
  if (!clientInstance) {
    clientInstance = new OnvmClient({
      rpcUrl: RPC_ENDPOINT,
      programId: PROGRAM_ID,
      projectId: PROJECT_ID,
      projectSecret: PROJECT_SECRET,
      timeout: 30000,
    })
  }
  return clientInstance
}

export async function POST(request: NextRequest) {
  const contentType = request.headers.get('content-type') || ''
  const client = getClient()
  
  if (!contentType.includes('application/json')) {
    try {
      const arrayBuffer = await request.arrayBuffer()
      const uint8Array = new Uint8Array(arrayBuffer)
      const result = await client.uploadBlob(uint8Array)
      console.log('Upload result:', result)
      return NextResponse.json(result)
    } catch (error) {
      console.error('ONVM API upload error:', error)
      return NextResponse.json(
        { error: error instanceof Error ? error.message : 'Upload failed' },
        { status: 500 }
      )
    }
  }
  
  try {
    const body = await request.json()
    const { operation, ...params } = body

    switch (operation) {
      case 'executeProgram': {
        const result = await client.executeProgram({
          program_id: params.program_id,
          input_base64: params.input_base64,
        })
        return NextResponse.json(result)
      }

      case 'downloadBlob': {
        const buffer = await client.downloadBlob(params.id)
        return new NextResponse(buffer, {
          headers: {
            'Content-Type': 'application/octet-stream',
          },
        })
      }

      default:
        return NextResponse.json(
          { error: 'Invalid operation' },
          { status: 400 }
        )
    }
  } catch (error) {
    console.error('ONVM API error:', error)
    return NextResponse.json(
      { error: error instanceof Error ? error.message : 'Unknown error' },
      { status: 500 }
    )
  }
}