import { PrismaClient } from '@prisma/client'

const prisma = new PrismaClient()

async function main() {
  console.log('🌱 Starting database seed...')

  // Example: Create some sample workflows if they don't exist
  const sampleWorkflows = [
    {
      name: 'example-workflow',
      description: 'A sample workflow for demonstration',
      flowId: 'sample-hash-1',
    },
  ]

  for (const workflow of sampleWorkflows) {
    const existing = await prisma.workflow.findUnique({
      where: { name: workflow.name },
    })

    if (!existing) {
      const created = await prisma.workflow.create({
        data: workflow,
      })
      console.log(`✅ Created workflow: ${created.name}`)

      // Create some sample labels
      await prisma.workflowLabel.create({
        data: {
          workflowName: created.name,
          label: 'latest',
          flowId: workflow.flowId,
        },
      })
      console.log(`📋 Created label 'latest' for ${created.name}`)
    } else {
      console.log(`⏭️  Workflow '${workflow.name}' already exists, skipping`)
    }
  }

  console.log('🎉 Database seed completed successfully')
}

main()
  .catch((e) => {
    console.error('❌ Error during seed:', e)
    process.exit(1)
  })
  .finally(async () => {
    await prisma.$disconnect()
  })