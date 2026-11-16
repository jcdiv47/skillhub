// SkillHub manages agent skills across the projects of the machine: the
// skills of .agents/skills, .claude/skills and pi, global and per
// project, with their links.
package main

import (
	"context"
	"log"
	"os"
	"path/filepath"
	"time"

	"github.com/egoist/mygo"
	"github.com/egoist/mygo/ui"
)

func main() {
	home, err := os.UserHomeDir()
	if err != nil {
		log.Fatal(err)
	}
	var win *mygo.Window
	// post runs fn on the main thread and draws the window again. Work
	// that ends before the window exists, as the first scan, still runs.
	post := func(fn func()) {
		mygo.RunOnMain(func() {
			fn()
			if win != nil {
				win.Invalidate()
			}
		})
	}
	w := &watcher{post: post}
	mygo.App.WhenReady(func() {
		dir, err := mygo.App.Path(mygo.PathUserData)
		if err != nil {
			dir = filepath.Join(home, ".config", "skillhub")
		}
		if v := os.Getenv("SKILLHUB_CONFIG"); v != "" {
			dir = v
		}
		a := newApp(env{
			home:       home,
			configPath: filepath.Join(dir, "config.json"),
			pickFolders: func(title string, multiple bool, done func([]string)) {
				go func() {
					paths, err := mygo.Dialog.Open(mygo.OpenDialogOptions{
						Parent: win, Title: title, Directory: true, Multiple: multiple,
						ShowHiddenFiles: true, CreateDirectories: true,
					})
					if err != nil || len(paths) == 0 {
						return
					}
					post(func() { done(paths) })
				}()
			},
			trash:   mygo.Shell.TrashItem,
			reveal:  mygo.Shell.ShowItemInFolder,
			openURL: func(url string) { mygo.Shell.OpenExternal(url) },
			background: func(work func() func()) {
				go func() {
					done := work()
					post(done)
				}()
			},
			watch: w.watch,
		})
		win = mygo.NewWindow(mygo.WindowOptions{
			Title:     "SkillHub",
			Width:     1320,
			Height:    840,
			MinWidth:  900,
			MinHeight: 560,
			Content:   ui.View(a.view),
		})
		// What changed while the app was in the background shows as it
		// comes back.
		win.OnFocus(func() {
			if a.snap != nil && !a.scanning {
				a.reload()
				win.Invalidate()
			}
		})
		win.OnClose(func(e *mygo.CloseEvent) {
			if !a.confirmClose(mygo.App.Quit) {
				e.PreventDefault()
				win.Invalidate()
			}
		})
		mygo.App.OnWillQuit(func(*mygo.QuitEvent) { a.saveConfig() })
	})
	if err := mygo.App.Run(); err != nil {
		log.Fatal(err)
	}
}

// watcher watches the skills folders shown, and reads them again a moment
// after they change.
type watcher struct {
	post   func(func())
	cancel context.CancelFunc
	key    string
}

func (w *watcher) watch(dirs, roots []string, changed func()) {
	key := filepath.Join(append(append([]string{}, dirs...), roots...)...)
	if w.cancel != nil && key == w.key {
		return
	}
	if w.cancel != nil {
		w.cancel()
	}
	ctx, cancel := context.WithCancel(context.Background())
	w.cancel, w.key = cancel, key
	events := make(chan struct{}, 1)
	start := func(p string, recursive bool) {
		fw, err := mygo.WatchFiles(ctx, p, mygo.FileWatchOptions{Recursive: recursive, Debounce: 200 * time.Millisecond})
		if err != nil {
			return
		}
		go func() {
			for {
				if _, err := fw.Next(ctx); err != nil {
					return
				}
				select {
				case events <- struct{}{}:
				default:
				}
			}
		}()
	}
	go func() {
		for _, d := range dirs {
			start(d, true)
		}
		for _, r := range roots {
			start(r, false)
		}
		for {
			select {
			case <-ctx.Done():
				return
			case <-events:
				w.post(changed)
				// Let a burst of changes settle before reading again.
				select {
				case <-ctx.Done():
					return
				case <-time.After(300 * time.Millisecond):
				}
			}
		}
	}()
}
