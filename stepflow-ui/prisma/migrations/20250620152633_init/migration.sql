-- CreateTable
CREATE TABLE "workflows" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "name" TEXT NOT NULL,
    "description" TEXT,
    "flow_hash" TEXT NOT NULL,
    "created_at" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" DATETIME NOT NULL
);

-- CreateTable
CREATE TABLE "workflow_labels" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "workflow_name" TEXT NOT NULL,
    "label" TEXT NOT NULL,
    "flow_hash" TEXT NOT NULL,
    "created_at" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updated_at" DATETIME NOT NULL,
    CONSTRAINT "workflow_labels_workflow_name_fkey" FOREIGN KEY ("workflow_name") REFERENCES "workflows" ("name") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "workflow_executions" (
    "id" TEXT NOT NULL PRIMARY KEY,
    "workflow_name" TEXT NOT NULL,
    "label" TEXT,
    "flow_hash" TEXT NOT NULL,
    "status" TEXT NOT NULL,
    "debug" BOOLEAN NOT NULL DEFAULT false,
    "input" TEXT NOT NULL,
    "result" TEXT,
    "created_at" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "completed_at" DATETIME,
    CONSTRAINT "workflow_executions_workflow_name_fkey" FOREIGN KEY ("workflow_name") REFERENCES "workflows" ("name") ON DELETE CASCADE ON UPDATE CASCADE
);

-- CreateTable
CREATE TABLE "flow_cache" (
    "flow_hash" TEXT NOT NULL PRIMARY KEY,
    "definition" TEXT NOT NULL,
    "created_at" DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "accessed_at" DATETIME NOT NULL
);

-- CreateIndex
CREATE UNIQUE INDEX "workflows_name_key" ON "workflows"("name");

-- CreateIndex
CREATE UNIQUE INDEX "workflow_labels_workflow_name_label_key" ON "workflow_labels"("workflow_name", "label");
