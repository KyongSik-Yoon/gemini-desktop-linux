package localapi

import (
	"container/list"
	"sync"
	"time"
)

// singleflightGroup은 동일 키에 대한 동시 다발적 원격 호출을 1회로 병합한다.
type singleflightGroup[T any] struct {
	mu    sync.Mutex
	calls map[string]*sfCall[T]
}

type sfCall[T any] struct {
	done chan struct{}
	val  T
	err  error
}

func (g *singleflightGroup[T]) Do(key string, fn func() (T, error)) (T, error) {
	g.mu.Lock()
	if g.calls == nil {
		g.calls = make(map[string]*sfCall[T])
	}
	if c, ok := g.calls[key]; ok {
		g.mu.Unlock()
		<-c.done
		return c.val, c.err
	}
	c := &sfCall[T]{done: make(chan struct{})}
	g.calls[key] = c
	g.mu.Unlock()

	defer func() {
		g.mu.Lock()
		delete(g.calls, key)
		g.mu.Unlock()
		close(c.done)
	}()

	c.val, c.err = fn()
	return c.val, c.err
}

type cachedImage struct {
	data        []byte
	contentType string
}

type imageCacheEntry struct {
	key         string
	data        []byte
	contentType string
	storedAt    time.Time
}

// imageCache는 프록시된 원격 이미지를 메모리 한도 내에서 보관하는 TTL 기반 LRU 캐시다.
type imageCache struct {
	mu         sync.Mutex
	maxBytes   int
	maxEntries int
	ttl        time.Duration
	curBytes   int
	order      *list.List
	items      map[string]*list.Element
}

func newImageCache(maxBytes, maxEntries int, ttl time.Duration) *imageCache {
	return &imageCache{
		maxBytes:   maxBytes,
		maxEntries: maxEntries,
		ttl:        ttl,
		order:      list.New(),
		items:      make(map[string]*list.Element),
	}
}

func (c *imageCache) Get(key string) ([]byte, string, bool) {
	c.mu.Lock()
	defer c.mu.Unlock()

	elem, ok := c.items[key]
	if !ok {
		return nil, "", false
	}
	entry := elem.Value.(*imageCacheEntry)
	if c.ttl > 0 && time.Since(entry.storedAt) > c.ttl {
		c.removeElementLocked(elem)
		return nil, "", false
	}
	c.order.MoveToFront(elem)
	return entry.data, entry.contentType, true
}

func (c *imageCache) Put(key string, data []byte, contentType string) {
	if len(data) == 0 || len(data) > c.maxBytes/2 {
		return
	}
	c.mu.Lock()
	defer c.mu.Unlock()

	if elem, ok := c.items[key]; ok {
		entry := elem.Value.(*imageCacheEntry)
		c.curBytes -= len(entry.data)
		entry.data = data
		entry.contentType = contentType
		entry.storedAt = time.Now()
		c.curBytes += len(data)
		c.order.MoveToFront(elem)
	} else {
		entry := &imageCacheEntry{
			key:         key,
			data:        data,
			contentType: contentType,
			storedAt:    time.Now(),
		}
		elem := c.order.PushFront(entry)
		c.items[key] = elem
		c.curBytes += len(data)
	}

	for c.curBytes > c.maxBytes || len(c.items) > c.maxEntries {
		back := c.order.Back()
		if back == nil {
			break
		}
		c.removeElementLocked(back)
	}
}

func (c *imageCache) removeElementLocked(elem *list.Element) {
	entry := elem.Value.(*imageCacheEntry)
	delete(c.items, entry.key)
	c.curBytes -= len(entry.data)
	c.order.Remove(elem)
}
