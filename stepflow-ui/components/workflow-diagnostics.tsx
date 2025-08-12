'use client'

import { AlertTriangle, Ban, Info } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { DiagnosticLevel } from '@/stepflow-api-client/model/diagnostic-level'
import { Diagnostic, AnalysisResult } from '@/lib/api-types'
import { cn } from '@/lib/utils'

export interface DiagnosticsProps {
  diagnostics: Diagnostic[]
  className?: string
}

export function WorkflowDiagnostics({ diagnostics, className }: DiagnosticsProps) {
  // Filter out ignored diagnostics by default
  const visibleDiagnostics = diagnostics.filter(d => !d.ignore)
  
  if (visibleDiagnostics.length === 0) {
    return null
  }

  const fatalDiagnostics = visibleDiagnostics.filter(d => d.level === DiagnosticLevel.Fatal)
  const errorDiagnostics = visibleDiagnostics.filter(d => d.level === DiagnosticLevel.Error)
  const warningDiagnostics = visibleDiagnostics.filter(d => d.level === DiagnosticLevel.Warning)

  const getLevelIcon = (level: DiagnosticLevel) => {
    switch (level) {
      case DiagnosticLevel.Fatal:
        return <Ban className="h-4 w-4" />
      case DiagnosticLevel.Error:
        return <AlertTriangle className="h-4 w-4" />
      case DiagnosticLevel.Warning:
        return <Info className="h-4 w-4" />
    }
  }

  const getLevelBadgeVariant = (level: DiagnosticLevel) => {
    switch (level) {
      case DiagnosticLevel.Fatal:
        return 'destructive' as const
      case DiagnosticLevel.Error:
        return 'destructive' as const
      case DiagnosticLevel.Warning:
        return 'secondary' as const
    }
  }

  const DiagnosticItem = ({ diagnostic }: { diagnostic: Diagnostic }) => (
    <div className={cn(
      "mb-2 rounded-lg border p-4",
      diagnostic.level === DiagnosticLevel.Fatal || diagnostic.level === DiagnosticLevel.Error 
        ? "border-destructive bg-destructive/10" 
        : "border-border bg-muted/50"
    )}>
      <div className="flex items-center gap-2 mb-2">
        {getLevelIcon(diagnostic.level)}
        <Badge variant={getLevelBadgeVariant(diagnostic.level)} className="text-xs">
          {diagnostic.level.toUpperCase()}
        </Badge>
        {diagnostic.path.length > 0 && (
          <Badge variant="outline" className="text-xs">
            Path: {diagnostic.path.join('.')}
          </Badge>
        )}
      </div>
      <div className="text-sm">
        {diagnostic.text}
      </div>
    </div>
  )

  return (
    <Card className={className}>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <AlertTriangle className="h-5 w-5" />
          Workflow Validation
        </CardTitle>
        <CardDescription>
          {fatalDiagnostics.length > 0 && (
            <span className="text-destructive font-medium">
              {fatalDiagnostics.length} fatal issue{fatalDiagnostics.length !== 1 ? 's' : ''} prevent execution
            </span>
          )}
          {errorDiagnostics.length > 0 && (
            <span className="text-destructive">
              {fatalDiagnostics.length > 0 ? ', ' : ''}
              {errorDiagnostics.length} error{errorDiagnostics.length !== 1 ? 's' : ''}
            </span>
          )}
          {warningDiagnostics.length > 0 && (
            <span className="text-muted-foreground">
              {(fatalDiagnostics.length > 0 || errorDiagnostics.length > 0) ? ', ' : ''}
              {warningDiagnostics.length} warning{warningDiagnostics.length !== 1 ? 's' : ''}
            </span>
          )}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-2">
        {/* Fatal diagnostics first */}
        {fatalDiagnostics.map((diagnostic, index) => (
          <DiagnosticItem key={`fatal-${index}`} diagnostic={diagnostic} />
        ))}
        
        {/* Error diagnostics second */}
        {errorDiagnostics.map((diagnostic, index) => (
          <DiagnosticItem key={`error-${index}`} diagnostic={diagnostic} />
        ))}
        
        {/* Warning diagnostics last */}
        {warningDiagnostics.map((diagnostic, index) => (
          <DiagnosticItem key={`warning-${index}`} diagnostic={diagnostic} />
        ))}
      </CardContent>
    </Card>
  )
}

// Helper function to extract diagnostics from different response formats
export function extractDiagnostics(analysis: AnalysisResult | unknown): Diagnostic[] {
  // Handle new AnalysisResult format
  if (typeof analysis === 'object' && analysis !== null && 'diagnostics' in analysis) {
    const analysisResult = analysis as AnalysisResult
    if (analysisResult.diagnostics?.diagnostics) {
      return analysisResult.diagnostics.diagnostics
    }
  }

  // Handle legacy format with separate validationErrors and validationWarnings
  const diagnostics: Diagnostic[] = []
  const anyAnalysis = analysis as Record<string, unknown>
  
  if (Array.isArray(anyAnalysis?.validationErrors)) {
    anyAnalysis.validationErrors.forEach((error: unknown) => {
      if (typeof error === 'object' && error !== null) {
        const errorObj = error as Record<string, unknown>
        diagnostics.push({
          level: DiagnosticLevel.Error,
          text: typeof errorObj.message === 'string' ? errorObj.message : 'Unknown error',
          path: [], // Legacy format doesn't have path information
          ignore: false,
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          message: error as any, // DiagnosticMessage is complex, using original for now
        })
      }
    })
  }

  if (Array.isArray(anyAnalysis?.validationWarnings)) {
    anyAnalysis.validationWarnings.forEach((warning: unknown) => {
      if (typeof warning === 'object' && warning !== null) {
        const warningObj = warning as Record<string, unknown>
        diagnostics.push({
          level: DiagnosticLevel.Warning, 
          text: typeof warningObj.message === 'string' ? warningObj.message : 'Unknown warning',
          path: [], // Legacy format doesn't have path information
          ignore: false,
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          message: warning as any, // DiagnosticMessage is complex, using original for now
        })
      }
    })
  }

  return diagnostics
}