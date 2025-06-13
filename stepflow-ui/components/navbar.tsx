'use client'

import Link from 'next/link'
import { usePathname } from 'next/navigation'
import { Button } from '@/components/ui/button'
import { NavigationMenu, NavigationMenuContent, NavigationMenuItem, NavigationMenuLink, NavigationMenuList, NavigationMenuTrigger } from '@/components/ui/navigation-menu'
import { Play, Workflow } from 'lucide-react'
import { cn } from '@/lib/utils'

const navigationItems = [
  { name: 'Executions', href: '/executions' },
  { name: 'Endpoints', href: '/endpoints' },
  { name: 'Components', href: '/components' },
]

export function Navbar() {
  const pathname = usePathname()

  return (
    <header className="sticky top-0 z-50 w-full border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
      <div className="container flex h-14 items-center">
        <div className="mr-4 hidden md:flex">
          <Link href="/" className="mr-6 flex items-center space-x-2">
            <Workflow className="h-6 w-6" />
            <span className="hidden font-bold sm:inline-block">
              StepFlow
            </span>
          </Link>
          <nav className="flex items-center space-x-6 text-sm font-medium">
            {navigationItems.map((item) => (
              <Link
                key={item.href}
                href={item.href}
                className={cn(
                  "transition-colors hover:text-foreground/80",
                  pathname === item.href ? "text-foreground" : "text-foreground/60"
                )}
              >
                {item.name}
              </Link>
            ))}
          </nav>
        </div>
        
        {/* Mobile Navigation */}
        <div className="flex flex-1 items-center space-x-2 md:hidden">
          <Link href="/" className="flex items-center space-x-2">
            <Workflow className="h-6 w-6" />
            <span className="font-bold">StepFlow</span>
          </Link>
        </div>

        {/* Actions */}
        <div className="flex flex-1 items-center justify-between space-x-2 md:justify-end">
          <div className="w-full flex-1 md:w-auto md:flex-none">
            {/* Mobile menu for navigation items */}
            <div className="md:hidden">
              <NavigationMenu>
                <NavigationMenuList>
                  <NavigationMenuItem>
                    <NavigationMenuTrigger>Menu</NavigationMenuTrigger>
                    <NavigationMenuContent>
                      <div className="grid gap-3 p-4 w-48">
                        {navigationItems.map((item) => (
                          <NavigationMenuLink key={item.href} asChild>
                            <Link
                              href={item.href}
                              className={cn(
                                "block select-none space-y-1 rounded-md p-3 leading-none no-underline outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground",
                                pathname === item.href && "bg-accent"
                              )}
                            >
                              <div className="text-sm font-medium leading-none">{item.name}</div>
                            </Link>
                          </NavigationMenuLink>
                        ))}
                      </div>
                    </NavigationMenuContent>
                  </NavigationMenuItem>
                </NavigationMenuList>
              </NavigationMenu>
            </div>
          </div>
          <Button asChild>
            <Link href="/execute">
              <Play className="mr-2 h-4 w-4" />
              Execute
            </Link>
          </Button>
        </div>
      </div>
    </header>
  )
}