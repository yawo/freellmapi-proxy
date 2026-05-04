import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiFetch } from '@/lib/api'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { PageHeader } from '@/components/page-header'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Badge } from '@/components/ui/badge'
import type { Model } from '../../../shared/types'

export default function ModelsPage() {
  const queryClient = useQueryClient()
  const [formData, setFormData] = useState({
    platform: '',
    modelId: '',
    displayName: '',
    baseUrl: '',
    validateUrl: '',
    intelligenceRank: '10',
    speedRank: '10',
  })

  const { data: models = [], isLoading } = useQuery<Model[]>({
    queryKey: ['models'],
    queryFn: () => apiFetch('/api/models'),
  })

  const addModel = useMutation({
    mutationFn: (body: any) =>
      apiFetch('/api/models', { method: 'POST', body: JSON.stringify(body) }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['models'] })
      setFormData({
        platform: '',
        modelId: '',
        displayName: '',
        baseUrl: '',
        validateUrl: '',
        intelligenceRank: '10',
        speedRank: '10',
      })
    },
  })

  const deleteModel = useMutation({
    mutationFn: (id: number) => apiFetch(`/api/models/${id}`, { method: 'DELETE' }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['models'] }),
  })

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    if (!formData.platform || !formData.modelId || !formData.displayName) return
    
    addModel.mutate({
      ...formData,
      intelligenceRank: parseInt(formData.intelligenceRank),
      speedRank: parseInt(formData.speedRank),
      baseUrl: formData.baseUrl || undefined,
      validateUrl: formData.validateUrl || undefined,
    })
  }

  return (
    <div>
      <PageHeader
        title="Models"
        description="Configure LLM models and their provider endpoints."
      />

      <div className="space-y-8">
        <section>
          <h2 className="text-sm font-medium mb-3">Add a new model / provider</h2>
          <form onSubmit={handleSubmit} className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 rounded-lg border p-4 bg-card">
            <div className="space-y-1.5">
              <Label className="text-xs">Platform (lowercase)</Label>
              <Input
                value={formData.platform}
                onChange={e => setFormData({ ...formData, platform: e.target.value })}
                placeholder="e.g. deepseek, my_local"
                className="font-mono text-xs"
              />
            </div>
            <div className="space-y-1.5">
              <Label className="text-xs">Model ID</Label>
              <Input
                value={formData.modelId}
                onChange={e => setFormData({ ...formData, modelId: e.target.value })}
                placeholder="e.g. deepseek-chat"
                className="font-mono text-xs"
              />
            </div>
            <div className="space-y-1.5">
              <Label className="text-xs">Display Name</Label>
              <Input
                value={formData.displayName}
                onChange={e => setFormData({ ...formData, displayName: e.target.value })}
                placeholder="e.g. DeepSeek V3"
              />
            </div>
            <div className="space-y-1.5 lg:col-span-2">
              <Label className="text-xs">Base URL (OpenAI-compatible)</Label>
              <Input
                value={formData.baseUrl}
                onChange={e => setFormData({ ...formData, baseUrl: e.target.value })}
                placeholder="https://api.example.com/v1 (optional for hardcoded ones)"
                className="font-mono text-xs"
              />
            </div>
             <div className="space-y-1.5">
              <Label className="text-xs">Intelligence Rank (1=best)</Label>
              <Input
                type="number"
                value={formData.intelligenceRank}
                onChange={e => setFormData({ ...formData, intelligenceRank: e.target.value })}
              />
            </div>
            <div className="flex items-end gap-3 lg:col-span-3">
               <div className="space-y-1.5 flex-1">
                <Label className="text-xs">Validate URL</Label>
                <Input
                  value={formData.validateUrl}
                  onChange={e => setFormData({ ...formData, validateUrl: e.target.value })}
                  placeholder="https://api.example.com/v1/models (optional)"
                  className="font-mono text-xs"
                />
              </div>
              <Button type="submit" size="sm" className="px-8" disabled={addModel.isPending}>
                {addModel.isPending ? 'Adding…' : 'Add Model'}
              </Button>
            </div>
          </form>
          {addModel.isError && (
            <p className="text-destructive text-xs mt-2">{(addModel.error as Error).message}</p>
          )}
        </section>

        <section>
          <h2 className="text-sm font-medium mb-3">Model Registry</h2>
          <div className="rounded-lg border bg-card overflow-hidden">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Platform</TableHead>
                  <TableHead>Model ID</TableHead>
                  <TableHead>Display Name</TableHead>
                  <TableHead>Rank (I/S)</TableHead>
                  <TableHead>Base URL</TableHead>
                  <TableHead className="text-right">Actions</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {isLoading ? (
                  <TableRow><TableCell colSpan={6} className="text-center py-8 text-muted-foreground">Loading...</TableCell></TableRow>
                ) : models.length === 0 ? (
                  <TableRow><TableCell colSpan={6} className="text-center py-8 text-muted-foreground">No models found.</TableCell></TableRow>
                ) : (
                  models.map((m) => (
                    <TableRow key={m.id}>
                      <TableCell className="font-mono text-xs"><Badge variant="outline">{m.platform}</Badge></TableCell>
                      <TableCell className="font-mono text-xs">{m.modelId}</TableCell>
                      <TableCell className="font-medium">{m.displayName}</TableCell>
                      <TableCell className="text-xs">{m.intelligenceRank} / {m.speedRank}</TableCell>
                      <TableCell className="font-mono text-[10px] text-muted-foreground truncate max-w-[200px]">
                        {m.baseUrl || <span className="italic">hardcoded default</span>}
                      </TableCell>
                      <TableCell className="text-right">
                        <Button
                          variant="ghost"
                          size="xs"
                          className="text-muted-foreground hover:text-destructive"
                          onClick={() => deleteModel.mutate(m.id)}
                          disabled={deleteModel.isPending}
                        >
                          Delete
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))
                )}
              </TableBody>
            </Table>
          </div>
        </section>
      </div>
    </div>
  )
}
