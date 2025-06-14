'use client'

import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Plus, Upload, FileText, Loader2 } from 'lucide-react'
import { useCreateEndpoint } from '@/lib/hooks/use-api'
import type { Workflow } from '@/lib/api'

interface CreateEndpointDialogProps {
  trigger?: React.ReactNode
}

const EXAMPLE_WORKFLOW = `{
  "input_schema": {
    "type": "object",
    "properties": {
      "pattern": {
        "type": "string"
      },
      "text": {
        "type": "string"
      }
    }
  },
  "output_schema": {
    "type": "object",
    "properties": {
      "pattern_matches": {
        "type": "array"
      },
      "sentiment_score": {
        "type": "number"
      },
      "word_analysis": {
        "type": "object"
      }
    }
  },
  "steps": [
    {
      "id": "create_word_analysis_blob",
      "component": "builtins://put_blob",
      "input_schema": null,
      "output_schema": null,
      "input": {
        "data": {
          "input_schema": {
            "type": "object",
            "properties": {
              "text": {
                "type": "string"
              }
            },
            "required": [
              "text"
            ]
          },
          "code": "text = data['text'].lower()\nwords = text.split()\n\nword_count = len(words)\nchar_count = len(text.replace(' ', ''))\n\n# Count word lengths\nword_lengths = {}\nfor word in words:\n    length = len(word)\n    word_lengths[length] = word_lengths.get(length, 0) + 1\n\n# Find most common words\nword_freq = {}\nfor word in words:\n    word_freq[word] = word_freq.get(word, 0) + 1\n\nmost_common = sorted(word_freq.items(), key=lambda x: x[1], reverse=True)[:3]\n\nreturn {\n    'word_count': word_count,\n    'char_count': char_count,\n    'avg_word_length': round(char_count / word_count, 2) if word_count > 0 else 0,\n    'word_length_distribution': word_lengths,\n    'most_common_words': [{'word': word, 'count': count} for word, count in most_common]\n}\n"
        }
      }
    },
    {
      "id": "create_pattern_search_blob",
      "component": "builtins://put_blob",
      "input_schema": null,
      "output_schema": null,
      "input": {
        "data": {
          "input_schema": {
            "type": "object",
            "properties": {
              "text": {
                "type": "string"
              },
              "pattern": {
                "type": "string"
              }
            },
            "required": [
              "text",
              "pattern"
            ]
          },
          "code": "text = data['text']\npattern = data['pattern']\n\ntry:\n    matches = re.findall(pattern, text)\n    return [{'match': match, 'index': i} for i, match in enumerate(matches)]\nexcept:\n    return []\n"
        }
      }
    },
    {
      "id": "create_sentiment_analysis_blob",
      "component": "builtins://put_blob",
      "input_schema": null,
      "output_schema": null,
      "input": {
        "data": {
          "input_schema": {
            "type": "object",
            "properties": {
              "text": {
                "type": "string"
              }
            },
            "required": [
              "text"
            ]
          },
          "code": "# Simple sentiment analysis based on positive/negative words\npositive_words = ['good', 'great', 'excellent', 'amazing', 'wonderful', 'fantastic', 'love', 'like', 'happy', 'joy']\nnegative_words = ['bad', 'terrible', 'awful', 'horrible', 'hate', 'dislike', 'sad', 'angry', 'upset', 'disappointed']\n\ntext = data['text'].lower()\nwords = text.split()\n\npositive_count = sum(1 for word in words if word in positive_words)\nnegative_count = sum(1 for word in words if word in negative_words)\n\ntotal_sentiment_words = positive_count + negative_count\nif total_sentiment_words == 0:\n    return 0.0  # Neutral\n\n# Return score between -1 (very negative) and 1 (very positive)\nsentiment_score = (positive_count - negative_count) / total_sentiment_words\nreturn round(sentiment_score, 2)\n"
        }
      }
    },
    {
      "id": "word_analysis",
      "component": "python://udf",
      "input_schema": null,
      "output_schema": null,
      "input": {
        "blob_id": {
          "$from": {
            "step": "create_word_analysis_blob"
          },
          "path": "blob_id"
        },
        "input": {
          "text": {
            "$from": {
              "workflow": "input"
            },
            "path": "text"
          }
        }
      }
    },
    {
      "id": "pattern_search",
      "component": "python://udf",
      "input_schema": null,
      "output_schema": null,
      "input": {
        "blob_id": {
          "$from": {
            "step": "create_pattern_search_blob"
          },
          "path": "blob_id"
        },
        "input": {
          "text": {
            "$from": {
              "workflow": "input"
            },
            "path": "text"
          },
          "pattern": {
            "$from": {
              "workflow": "input"
            },
            "path": "pattern"
          }
        }
      }
    },
    {
      "id": "sentiment_analysis",
      "component": "python://udf",
      "input_schema": null,
      "output_schema": null,
      "input": {
        "blob_id": {
          "$from": {
            "step": "create_sentiment_analysis_blob"
          },
          "path": "blob_id"
        },
        "input": {
          "text": {
            "$from": {
              "workflow": "input"
            },
            "path": "text"
          }
        }
      }
    }
  ],
  "output": {
    "word_analysis": {
      "$from": {
        "step": "word_analysis"
      }
    },
    "pattern_matches": {
      "$from": {
        "step": "pattern_search"
      }
    },
    "sentiment_score": {
      "$from": {
        "step": "sentiment_analysis"
      }
    }
  }
}
`

export function CreateEndpointDialog({ trigger }: CreateEndpointDialogProps) {
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [label, setLabel] = useState('')
  const [workflowContent, setWorkflowContent] = useState(EXAMPLE_WORKFLOW)
  const [inputMethod, setInputMethod] = useState<'editor' | 'upload'>('editor')

  const createEndpointMutation = useCreateEndpoint()

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()

    if (!name.trim()) {
      return
    }

    try {
      let workflow: Workflow
      try {
        workflow = JSON.parse(workflowContent)
      } catch (error) {
        console.error('Invalid workflow JSON:', error)
        return
      }

      await createEndpointMutation.mutateAsync({
        name: name.trim(),
        data: { workflow },
        label: label.trim() || undefined
      })

      // Reset form and close dialog
      setName('')
      setLabel('')
      setWorkflowContent(EXAMPLE_WORKFLOW)
      setInputMethod('editor')
      setOpen(false)
    } catch (error) {
      console.error('Failed to create endpoint:', error)
    }
  }

  const loadExampleWorkflow = () => {
    setWorkflowContent(EXAMPLE_WORKFLOW)
  }

  const defaultTrigger = (
    <Button>
      <Plus className="mr-2 h-4 w-4" />
      Create Endpoint
    </Button>
  )

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        {trigger || defaultTrigger}
      </DialogTrigger>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>Create New Endpoint</DialogTitle>
            <DialogDescription>
              Create a named endpoint for your workflow. You can optionally specify a label for versioning.
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-6 py-4">
            {/* Endpoint Details */}
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="endpoint-name">
                  Endpoint Name <span className="text-red-500">*</span>
                </Label>
                <Input
                  id="endpoint-name"
                  placeholder="my-workflow"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  required
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="endpoint-label">
                  Label (Optional)
                </Label>
                <Input
                  id="endpoint-label"
                  placeholder="v1.0, stable, beta..."
                  value={label}
                  onChange={(e) => setLabel(e.target.value)}
                />
                <p className="text-xs text-muted-foreground">
                  Leave empty to create the default version
                </p>
              </div>
            </div>

            {/* Workflow Definition */}
            <div className="space-y-4">
              <Label>Workflow Definition</Label>
              <Tabs value={inputMethod} onValueChange={(value) => setInputMethod(value as 'editor' | 'upload')}>
                <TabsList className="grid w-full grid-cols-2">
                  <TabsTrigger value="editor">
                    <FileText className="mr-2 h-4 w-4" />
                    Editor
                  </TabsTrigger>
                  <TabsTrigger value="upload">
                    <Upload className="mr-2 h-4 w-4" />
                    Upload File
                  </TabsTrigger>
                </TabsList>

                <TabsContent value="editor" className="space-y-4">
                  <div className="space-y-2">
                    <div className="flex items-center justify-between">
                      <Label htmlFor="workflow-content">JSON Workflow Definition</Label>
                      <Button type="button" variant="outline" size="sm" onClick={loadExampleWorkflow}>
                        <FileText className="mr-2 h-3 w-3" />
                        Load Example
                      </Button>
                    </div>
                    <Textarea
                      id="workflow-content"
                      placeholder="Paste your workflow JSON here..."
                      className="min-h-[300px] font-mono text-sm"
                      value={workflowContent}
                      onChange={(e) => setWorkflowContent(e.target.value)}
                      required
                    />
                  </div>
                </TabsContent>

                <TabsContent value="upload" className="space-y-4">
                  <div className="border-2 border-dashed border-muted-foreground/25 rounded-lg p-8 text-center">
                    <Upload className="mx-auto h-12 w-12 text-muted-foreground/50" />
                    <p className="mt-2 text-sm text-muted-foreground">
                      Drag and drop a JSON file here, or click to browse
                    </p>
                    <Input
                      type="file"
                      accept=".json,.yaml,.yml"
                      className="mt-4"
                      onChange={(e) => {
                        const file = e.target.files?.[0]
                        if (file) {
                          const reader = new FileReader()
                          reader.onload = (event) => {
                            setWorkflowContent(event.target?.result as string)
                          }
                          reader.readAsText(file)
                        }
                      }}
                    />
                  </div>
                </TabsContent>
              </Tabs>
            </div>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button
              type="submit"
              disabled={!name.trim() || !workflowContent.trim() || createEndpointMutation.isPending}
            >
              {createEndpointMutation.isPending ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  Creating...
                </>
              ) : (
                <>
                  <Plus className="mr-2 h-4 w-4" />
                  Create Endpoint
                </>
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}