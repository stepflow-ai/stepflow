import { cn } from '@/lib/utils'

describe('Utility Functions', () => {
  describe('cn (className merger)', () => {
    it('should merge simple class names', () => {
      expect(cn('class1', 'class2')).toBe('class1 class2')
    })

    it('should handle conditional classes', () => {
      expect(cn('base', true && 'conditional', false && 'hidden')).toBe('base conditional')
    })

    it('should merge Tailwind classes and resolve conflicts', () => {
      // twMerge should resolve conflicting Tailwind classes
      expect(cn('px-2', 'px-4')).toBe('px-4')
      expect(cn('text-red-500', 'text-blue-500')).toBe('text-blue-500')
    })

    it('should handle arrays of classes', () => {
      expect(cn(['class1', 'class2'], 'class3')).toBe('class1 class2 class3')
    })

    it('should handle objects with boolean values', () => {
      expect(cn({
        'always-present': true,
        'conditionally-present': true,
        'never-present': false
      })).toBe('always-present conditionally-present')
    })

    it('should handle mixed input types', () => {
      expect(cn(
        'base',
        ['array1', 'array2'],
        {
          'object-true': true,
          'object-false': false
        },
        true && 'conditional',
        'final'
      )).toBe('base array1 array2 object-true conditional final')
    })

    it('should handle empty and undefined inputs', () => {
      expect(cn()).toBe('')
      expect(cn(undefined, null, '', 'valid')).toBe('valid')
    })
  })
})