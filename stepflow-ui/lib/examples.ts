import { Flow } from '../stepflow-api-client/model/flow'

// TODO: Replace all uses of `Workflow` with `Flow` throughout the codebase
// This alias is temporary to ease the migration
type Workflow = Flow

export interface InputExample {
  name: string
  description?: string
  data: Record<string, unknown>
  format?: 'json' | 'yaml'
}

export interface WorkflowExample {
  name: string
  description?: string
  workflow: Workflow
  inputExamples: InputExample[]
}

// Common input examples that can be used across different workflows
export const COMMON_INPUT_EXAMPLES: InputExample[] = [
  {
    name: 'Python Math Example',
    description: 'Simple arithmetic operations with two numbers',
    data: { m: 8, n: 5 },
    format: 'json'
  },
  {
    name: 'Text Processing',
    description: 'Text analysis with pattern matching',
    data: {
      text: "This is a great example of text processing with positive sentiment.",
      pattern: "\\b\\w+ing\\b"
    },
    format: 'json'
  },
  {
    name: 'OpenAI Chat',
    description: 'AI chat completion request',
    data: {
      prompt: "Explain quantum computing in simple terms.",
      system_message: "You are a helpful expert explaining complex topics to beginners."
    },
    format: 'json'
  },
  {
    name: 'File Processing',
    description: 'File loading and processing workflow',
    data: {
      file_path: "/path/to/input/file.txt",
      operation: "analyze"
    },
    format: 'json'
  },
  {
    name: 'Data Pipeline',
    description: 'Data transformation and analysis',
    data: {
      data_source: "sales_data.json",
      filters: ["completed", "paid"],
      aggregations: ["sum", "count"]
    },
    format: 'json'
  }
]

// Example workflows that demonstrate common patterns
export const EXAMPLE_WORKFLOWS: WorkflowExample[] = [
  {
    name: 'Python Math Operations',
    description: 'Demonstrates basic arithmetic operations using Python components',
    workflow: {
      name: 'Math Operations',
      description: 'Performs addition, multiplication, and compound operations',
      inputSchema: {
        type: 'object',
        properties: {
          m: { type: 'integer' },
          n: { type: 'integer' }
        },
        required: ['m', 'n']
      },
      steps: [
        {
          id: 'm_plus_n',
          component: 'python://add',
          input: {
            a: { $from: { workflow: 'input' }, path: 'm' },
            b: { $from: { workflow: 'input' }, path: 'n' }
          }
        },
        {
          id: 'm_times_n',
          component: 'python://multiply',
          input: {
            a: { $from: { workflow: 'input' }, path: 'm' },
            b: { $from: { workflow: 'input' }, path: 'n' }
          }
        },
        {
          id: 'm_plus_n_times_n',
          component: 'python://multiply',
          input: {
            a: { $from: { step: 'm_plus_n' }, path: 'result' },
            b: { $from: { workflow: 'input' }, path: 'n' }
          }
        }
      ],
      output: {
        m_plus_n_times_n: { $from: { step: 'm_plus_n_times_n' }, path: 'result' },
        m_times_n: { $from: { step: 'm_times_n' }, path: 'result' },
        m_plus_n: { $from: { step: 'm_plus_n' }, path: 'result' }
      }
    },
    inputExamples: [
      {
        name: 'Basic Math',
        description: 'Simple numbers for arithmetic operations',
        data: { m: 8, n: 5 },
        format: 'json'
      },
      {
        name: 'Large Numbers',
        description: 'Testing with larger values',
        data: { m: 100, n: 25 },
        format: 'json'
      }
    ]
  },
  {
    name: 'Text Analysis Pipeline',
    description: 'Comprehensive text processing with multiple analysis steps',
    workflow: {
      name: 'Text Analysis',
      description: 'Analyzes text for word patterns, sentiment, and statistics',
      inputSchema: {
        type: 'object',
        properties: {
          text: { type: 'string' },
          pattern: { type: 'string' }
        },
        required: ['text']
      },
      steps: [
        {
          id: 'create_word_analysis_blob',
          component: 'builtins://put_blob',
          input: {
            data: {
              inputSchema: {
                type: 'object',
                properties: { text: { type: 'string' } },
                required: ['text']
              },
              code: `
text = data['text'].lower()
words = text.split()
word_count = len(words)
char_count = len(text.replace(' ', ''))
word_lengths = {}
for word in words:
    length = len(word)
    word_lengths[length] = word_lengths.get(length, 0) + 1
word_freq = {}
for word in words:
    word_freq[word] = word_freq.get(word, 0) + 1
most_common = sorted(word_freq.items(), key=lambda x: x[1], reverse=True)[:3]
return {
    'word_count': word_count,
    'char_count': char_count,
    'avg_word_length': round(char_count / word_count, 2) if word_count > 0 else 0,
    'word_length_distribution': word_lengths,
    'most_common_words': [{'word': word, 'count': count} for word, count in most_common]
}
              `
            }
          }
        },
        {
          id: 'word_analysis',
          component: 'python://udf',
          input: {
            blob_id: { $from: { step: 'create_word_analysis_blob' }, path: 'blob_id' },
            input: {
              text: { $from: { workflow: 'input' }, path: 'text' }
            }
          }
        }
      ],
      output: {
        word_analysis: { $from: { step: 'word_analysis' } }
      }
    },
    inputExamples: [
      {
        name: 'Short Text',
        description: 'Brief text for analysis',
        data: {
          text: "This is a great example of text processing with positive sentiment.",
          pattern: "\\b\\w+ing\\b"
        },
        format: 'json'
      },
      {
        name: 'Longer Text',
        description: 'More comprehensive text sample',
        data: {
          text: "Machine learning is transforming how we process and analyze data. It enables computers to learn patterns from data automatically, making predictions and decisions without being explicitly programmed for each task.",
          pattern: "\\b\\w+ing\\b"
        },
        format: 'json'
      }
    ]
  },
  {
    name: 'Simple Expression Evaluation',
    description: 'Evaluates mathematical expressions using the eval component',
    workflow: {
      name: 'Expression Evaluator',
      description: 'Evaluates mathematical expressions and returns results',
      inputSchema: {
        type: 'object',
        properties: {
          expression: { type: 'string' },
          variables: { type: 'object' }
        },
        required: ['expression']
      },
      steps: [
        {
          id: 'evaluate',
          component: 'builtins://eval',
          input: {
            expression: { $from: { workflow: 'input' }, path: 'expression' },
            context: { $from: { workflow: 'input' }, path: 'variables' }
          }
        }
      ],
      output: {
        result: { $from: { step: 'evaluate' }, path: 'result' }
      }
    },
    inputExamples: [
      {
        name: 'Simple Math',
        description: 'Basic arithmetic expression',
        data: {
          expression: "2 + 3 * 4",
          variables: {}
        },
        format: 'json'
      },
      {
        name: 'With Variables',
        description: 'Expression using variables',
        data: {
          expression: "x * y + z",
          variables: { x: 10, y: 5, z: 3 }
        },
        format: 'json'
      }
    ]
  }
]

// Get input examples for a specific workflow type or use common examples
export function getInputExamplesForWorkflow(workflowName?: string): InputExample[] {
  if (workflowName) {
    const workflowExample = EXAMPLE_WORKFLOWS.find(w => 
      w.name.toLowerCase().includes(workflowName.toLowerCase()) ||
      w.workflow.name?.toLowerCase().includes(workflowName.toLowerCase())
    )
    if (workflowExample) {
      return workflowExample.inputExamples
    }
  }
  return COMMON_INPUT_EXAMPLES
}

// Get a specific example workflow by name
export function getExampleWorkflow(name: string): WorkflowExample | undefined {
  return EXAMPLE_WORKFLOWS.find(w => w.name === name)
}

// Get examples based on workflow components
export function getInputExamplesForComponents(components: string[]): InputExample[] {
  const examples: InputExample[] = []
  
  // Add specific examples based on components present
  if (components.some(c => c.includes('python') && (c.includes('add') || c.includes('multiply')))) {
    examples.push(COMMON_INPUT_EXAMPLES[0]) // Python Math Example
  }
  
  if (components.some(c => c.includes('openai'))) {
    examples.push(COMMON_INPUT_EXAMPLES[2]) // OpenAI Chat
  }
  
  if (components.some(c => c.includes('load_file') || c.includes('file'))) {
    examples.push(COMMON_INPUT_EXAMPLES[3]) // File Processing
  }
  
  if (components.some(c => c.includes('udf') || c.includes('text'))) {
    examples.push(COMMON_INPUT_EXAMPLES[1]) // Text Processing
  }
  
  // If no specific examples found, return common examples
  if (examples.length === 0) {
    return COMMON_INPUT_EXAMPLES.slice(0, 3)
  }
  
  return examples
}