// 博客后台根视图：管理导航、状态协调与 mutation 定义。
// 渲染细节由 PostEditor、TaxonomyPanel 和 BlogHomePanel 等子组件负责。

import { useEffect, useMemo, useState } from "react";
import {
  Check,
  CircleHelp,
  FileText,
  Home,
  Plus,
  Save,
  Search,
  Square,
  Star,
  Tags as TagsIcon,
  Trash2
} from "lucide-react";
import {
  type QueryClient,
  useMutation,
  useQuery,
  useQueryClient
} from "@tanstack/react-query";
import { api } from "../api";
import { queryKeys } from "../query-keys";
import type { BlogHomeConfig, BlogPost, BlogPostSaveRequest } from "../types";
import {
  ActionButton,
  CardActions,
  CardInner,
  CardRow,
  InlineNotice,
  LoadingBlock,
  MutationError,
  SectionHeader,
  TickerText,
  TruncatedText
} from "../components/ui";
import { PostEditor } from "./PostEditor";
import { TaxonomyPanel } from "./TaxonomyPanel";
import {
  type BlogAdminSection,
  type BlogDraft,
  type BlogPostSection,
  type BlogStatusFilter,
  emptyBlogDraft,
  emptyBlogHomeConfig,
  filterTagsForCategory,
  invalidateBlogQueries,
  matchesBlogFilters,
  parseTagsText,
  toBlogAssetDisplayPath,
  toBlogDraft,
  toBlogHomeDraft,
  toBlogHomeSaveRequest,
  toBlogPostSaveRequest,
  DEFAULT_BLOG_IMAGE_PATH,
  DEFAULT_BLOG_IMAGE_DISPLAY_PATH
} from "./blog-types";

// ─── 博客主页配置面板 ─────────────────────────────────────────────────────────

function BlogHomePanel({
  draft, onUpdate, isLoading, loadError, saveError, isSaved
}: {
  draft: BlogHomeConfig;
  onUpdate: (patch: Partial<BlogHomeConfig>) => void;
  isLoading: boolean;
  loadError: Error | null;
  saveError: Error | null;
  isSaved: boolean;
}) {
  return (
    <>
      {isLoading ? <LoadingBlock label="正在读取主页配置" /> : null}
      {loadError ? <InlineNotice tone="danger" text={loadError.message} /> : null}
      {saveError ? <InlineNotice tone="danger" text={saveError.message} /> : null}
      {isSaved && (
        <InlineNotice tone="warn" text="已保存，blog 正在后台重新构建，稍后自动刷新。" />
      )}

      <div className="content-grid blog-nav-grid">
        {/* 内容块 1：五个短文字字段，每行一个 */}
        <article className="content-card blog-home-card">
          <CardInner>
            {([
              { key: "site_url",      label: "地址" },
              { key: "site_name",     label: "名称" },
              { key: "site_title",    label: "站标" },
              { key: "hero_title",    label: "主标" },
              { key: "hero_subtitle", label: "副标" },
            ] as const).map(({ key, label }) => (
              <CardRow key={key} label={label}>
                <input
                  value={draft[key]}
                  onChange={e => onUpdate({ [key]: e.target.value })}
                  className="blog-home-input"
                />
              </CardRow>
            ))}
          </CardInner>
        </article>

        {/* 内容块 2：背景图、头像，每行一个 */}
        <article className="content-card blog-home-card">
          <CardInner>
            <CardRow label="背景">
              <input value={draft.background_image} onChange={e => onUpdate({ background_image: e.target.value })} className="blog-home-input" />
            </CardRow>
            <CardRow label="头像">
              <input value={draft.avatar_image} onChange={e => onUpdate({ avatar_image: e.target.value })} className="blog-home-input" />
            </CardRow>
          </CardInner>
        </article>

        {/* 内容块 3-5：站点简介、公告、页脚说明（五行全用来写） */}
        {([
          { key: "site_description", label: "简介" },
          { key: "announcement",     label: "公告" },
          { key: "footer_note",      label: "页脚" },
        ] as const).map(({ key, label }) => (
          <article className="content-card blog-home-card" key={key}>
            <div className="blog-home-text-block">
              <span className="blog-home-text-label">{label}</span>
              <textarea
                value={draft[key]}
                onChange={e => onUpdate({ [key]: e.target.value })}
                className="blog-home-textarea"
              />
            </div>
          </article>
        ))}
      </div>
    </>
  );
}

// ─── 文章筛选工具栏 ───────────────────────────────────────────────────────────

function BlogPostFilterToolbar({
  query, categoryFilter, featuredOnly, categories, onQueryChange, onCategoryChange, onFeaturedOnlyChange
}: {
  query: string; categoryFilter: string; featuredOnly: boolean;
  categories: Array<{ name: string; count: number }>;
  onQueryChange: (v: string) => void; onCategoryChange: (v: string) => void; onFeaturedOnlyChange: (v: boolean) => void;
}) {
  return (
    <div className="blog-filter-toolbar">
      <label className="blog-inline-filter">
        <span>分类</span>
        <select value={categoryFilter} onChange={(e) => onCategoryChange(e.target.value)}>
          <option value="all">全部分类</option>
          <option value="__uncategorized__">未分类</option>
          {categories.map((c) => <option key={c.name} value={c.name}>{c.name}</option>)}
        </select>
      </label>
      <label className="check-field blog-featured-filter">
        <input type="checkbox" checked={featuredOnly} onChange={(e) => onFeaturedOnlyChange(e.target.checked)} />
        <span>精选</span>
      </label>
      <label className="blog-inline-filter blog-inline-search">
        <span>筛选</span>
        <span className="search-box">
          <Search size={16} />
          <input type="search" value={query} onChange={(e) => onQueryChange(e.target.value)} placeholder="搜索标题、路径、标签" />
        </span>
      </label>
    </div>
  );
}

// ─── 文章表格 ─────────────────────────────────────────────────────────────────

function PostTable({
  posts, loading, selectedPath, onSelect, onUnpublish, onDelete, unpublishPending, deletePending
}: {
  posts: BlogPost[]; loading: boolean; selectedPath: string | null;
  onSelect: (path: string) => void; onUnpublish: (path: string) => void; onDelete: (path: string) => void;
  unpublishPending?: boolean; deletePending?: boolean;
}) {
  if (loading) return <LoadingBlock label="正在读取文章" />;
  if (!posts.length) {
    return (
      <div className="empty-state">
        <CircleHelp size={20} />
        <span>没有找到文章</span>
      </div>
    );
  }
  return (
    <div className="content-grid blog-post-list">
      {posts.map((post) => (
        <article
          className={selectedPath === post.relative_path ? "content-card blog-post-list-item active" : "content-card blog-post-list-item"}
          key={post.relative_path}
        >
          <CardInner>
            <CardRow label="标题">
              <TruncatedText>
                <TickerText>{post.title}</TickerText>
              </TruncatedText>
            </CardRow>
            <CardRow label="分类">
              <span className="blog-post-list-meta">
                <TickerText>{post.category ?? "未分类"}</TickerText>
              </span>
            </CardRow>
            <CardRow label="标签" span={3}>
              <span className="blog-post-list-tags">
                {post.tags.map((tag) => <em key={tag}>{tag}</em>)}
              </span>
            </CardRow>
            <CardActions>
                <button type="button" className="card-action-button primary" onClick={() => onSelect(post.relative_path)}>
                  <FileText size={12} /><span>编辑</span>
                </button>
                <button type="button" className="card-action-button" disabled={post.draft || unpublishPending} onClick={() => onUnpublish(post.relative_path)}>
                  <Square size={12} /><span>下线</span>
                </button>
                <button type="button" className="card-action-button danger" disabled={deletePending} onClick={() => onDelete(post.relative_path)}>
                  <Trash2 size={12} /><span>删除</span>
                </button>
            </CardActions>
          </CardInner>
        </article>
      ))}
    </div>
  );
}

// ─── BlogView ─────────────────────────────────────────────────────────────────

export function BlogView() {
  const queryClient = useQueryClient();
  const [query, setQuery] = useState("");
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [isNewPost, setIsNewPost] = useState(false);
  const [draft, setDraft] = useState<BlogDraft>(() => emptyBlogDraft());
  const [categoryFilter, setCategoryFilter] = useState("all");
  const [featuredOnly, setFeaturedOnly] = useState(false);

  const homeQuery = useQuery({ queryKey: queryKeys.blog.home, queryFn: api.blogHome });
  const [homeDraft, setHomeDraft] = useState<BlogHomeConfig>(() => emptyBlogHomeConfig());
  const updateHomeDraft = (patch: Partial<BlogHomeConfig>) => setHomeDraft((cur) => ({ ...cur, ...patch }));
  const saveHomeMutation = useMutation({
    mutationFn: api.saveBlogHome,
    onSuccess: async (data) => {
      setHomeDraft(toBlogHomeDraft(data));
      await queryClient.invalidateQueries({ queryKey: queryKeys.blog.home });
    }
  });

  const postsQuery = useQuery({ queryKey: queryKeys.blog.posts, queryFn: api.blogPosts });
  const taxonomyQuery = useQuery({ queryKey: queryKeys.blog.taxonomy, queryFn: api.blogTaxonomy });
  const detailQuery = useQuery({
    queryKey: queryKeys.blog.detail(selectedPath),
    queryFn: () => api.blogPostDetail(selectedPath ?? ""),
    enabled: Boolean(selectedPath) && !isNewPost
  });

  // ─── Mutations ──────────────────────────────────────────────────────────────

  const saveMutation = useMutation({
    mutationFn: api.saveBlogPost,
    onSuccess: async (data) => {
      setIsNewPost(false);
      setSelectedPath(data.post.relative_path);
      setDraft((cur) => ({
        ...cur,
        original_relative_path: data.post.relative_path,
        relative_path: data.post.relative_path,
        hero_image: toBlogAssetDisplayPath(data.post.hero_image)
      }));
      await invalidateBlogQueries(queryClient);
    }
  });

  const unpublishMutation = useMutation({
    mutationFn: api.unpublishPost,
    onSuccess: async () => { await invalidateBlogQueries(queryClient); }
  });

  const deleteMutation = useMutation({
    mutationFn: api.deleteBlogPost,
    onSuccess: async () => {
      setSelectedPath(null); setIsNewPost(false); setDraft(emptyBlogDraft());
      await invalidateBlogQueries(queryClient);
    }
  });

  const addTagMutation = useMutation({
    mutationFn: ({ name, category }: { name: string; category: string }) => api.addBlogTag(name, category),
    onSuccess: async () => { await invalidateBlogQueries(queryClient); }
  });

  const renameTagMutation = useMutation({
    mutationFn: ({ from, to, category }: { from: string; to: string; category: string }) => api.renameBlogTag(from, to, category),
    onSuccess: async (_data, { from, to, category }) => {
      if (!category || draft.category === category) renameTagInDraft(from, to);
      await invalidateBlogQueries(queryClient);
    }
  });

  const deleteTagMutation = useMutation({
    mutationFn: ({ tag, category }: { tag: string; category: string }) => api.deleteBlogTag(tag, category),
    onSuccess: async (_data, { tag, category }) => {
      if (!category || draft.category === category) removeTagFromDraft(tag);
      await invalidateBlogQueries(queryClient);
    }
  });

  const addCategoryMutation = useMutation({
    mutationFn: api.addBlogCategory,
    onSuccess: async (_data, name) => {
      const category = name.trim();
      if (category) { setDraft((cur) => ({ ...cur, category, tagsText: "" })); setCategoryFilter(category); }
      await invalidateBlogQueries(queryClient);
    }
  });

  const renameCategoryMutation = useMutation({
    mutationFn: ({ from, to }: { from: string; to: string }) => api.renameBlogCategory(from, to),
    onSuccess: async (_data, { from, to }) => {
      const next = to.trim();
      setDraft((cur) => cur.category === from ? { ...cur, category: next } : cur);
      setCategoryFilter((cur) => cur === from ? next : cur);
      await invalidateBlogQueries(queryClient);
    }
  });

  const deleteCategoryMutation = useMutation({
    mutationFn: api.deleteBlogCategory,
    onSuccess: async (_data, category) => {
      setDraft((cur) => cur.category === category ? { ...cur, category: "", tagsText: "" } : cur);
      setCategoryFilter((cur) => cur === category ? "all" : cur);
      await invalidateBlogQueries(queryClient);
    }
  });

  // ─── Effects ────────────────────────────────────────────────────────────────

  useEffect(() => {
    if (homeQuery.data) setHomeDraft(toBlogHomeDraft(homeQuery.data));
  }, [homeQuery.data]);

  useEffect(() => {
    if (!detailQuery.data || isNewPost) return;
    setDraft(toBlogDraft(detailQuery.data));
  }, [detailQuery.data, isNewPost]);

  // ─── Derived state ──────────────────────────────────────────────────────────

  const allPosts = postsQuery.data ?? [];
  const featuredCount = allPosts.filter((p) => p.featured).length;
  const publishedCount = allPosts.filter((p) => !p.draft).length;
  const draftCount = allPosts.filter((p) => p.draft).length;
  const categoryCount = (taxonomyQuery.data?.categories ?? []).length;

  const categoryTagMap = useMemo(() => {
    const map = new Map<string, Array<{ name: string; count: number }>>();
    for (const group of taxonomyQuery.data?.category_tags ?? []) {
      map.set(group.category, group.tags);
    }
    return map;
  }, [taxonomyQuery.data?.category_tags]);

  const draftCategory = draft.category.trim();
  const draftCategoryTags = draftCategory ? (categoryTagMap.get(draftCategory) ?? []) : [];
  const selectedTags = useMemo(() => parseTagsText(draft.tagsText), [draft.tagsText]);

  const publishedPosts = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return allPosts.filter((p) => !p.draft && matchesBlogFilters(p, normalized, "published", categoryFilter, featuredOnly));
  }, [allPosts, query, categoryFilter, featuredOnly]);

  const draftPosts = useMemo(() =>
    allPosts.filter((p) => p.draft),
    [allPosts]
  );

  const categoryBlocks = useMemo(
    () => (taxonomyQuery.data?.categories ?? []).map((c) => ({ ...c, tags: categoryTagMap.get(c.name) ?? [] })),
    [categoryTagMap, taxonomyQuery.data?.categories]
  );

  const isEditing = selectedPath !== null || isNewPost;

  // ─── Handlers ───────────────────────────────────────────────────────────────

  const savePayload = (): BlogPostSaveRequest =>
    toBlogPostSaveRequest({ ...draft, tagsText: filterTagsForCategory(draft.category, draft.tagsText, categoryTagMap) });

  const startNewPost = () => {
    setIsNewPost(true); setSelectedPath(null); setDraft(emptyBlogDraft());
  };

  const updateDraft = (patch: Partial<BlogDraft>) => setDraft((cur) => ({ ...cur, ...patch }));

  const selectDraftCategory = (category: string) =>
    setDraft((cur) => ({ ...cur, category, tagsText: filterTagsForCategory(category, cur.tagsText, categoryTagMap) }));

  const toggleTagInDraft = (tag: string) => {
    const nextTag = tag.trim();
    if (!nextTag) return;
    setDraft((cur) => {
      const category = cur.category.trim();
      const allowedTags = new Set((categoryTagMap.get(category) ?? []).map((t) => t.name));
      if (!category || !allowedTags.has(nextTag)) return cur;
      const tags = parseTagsText(cur.tagsText);
      if (tags.includes(nextTag)) return { ...cur, tagsText: tags.filter((t) => t !== nextTag).join(", ") };
      return { ...cur, tagsText: [...tags, nextTag].join(", ") };
    });
  };

  const removeTagFromDraft = (tag: string) =>
    setDraft((cur) => ({ ...cur, tagsText: parseTagsText(cur.tagsText).filter((t) => t !== tag).join(", ") }));

  const renameTagInDraft = (from: string, to: string) => {
    const nextTag = to.trim();
    if (!from.trim() || !nextTag) return;
    setDraft((cur) => ({
      ...cur,
      tagsText: Array.from(new Set(parseTagsText(cur.tagsText).map((t) => (t === from ? nextTag : t)))).join(", ")
    }));
  };

  const saveWithDraftState = (asDraft: boolean) => {
    setDraft((cur) => ({ ...cur, draft: asDraft }));
    saveMutation.mutate({ ...savePayload(), draft: asDraft });
  };

  const unpublishPost = (path: string) => { if (path) unpublishMutation.mutate(path); };
  const deletePost = (path: string) => {
    if (path && window.confirm(`确定删除文章 ${path}？`)) deleteMutation.mutate(path);
  };

  // ─── Render（格式同总览：每个 section-band = 一行标题 + 下方内容块）────────────

  return (
    <section className="view-stack">

      {/* ── 编辑器（选中文章或新建时置顶显示）────────────────────────────── */}
      {isEditing && (
        <section className="section-band">
          <SectionHeader
            icon={isNewPost ? Plus : FileText}
            title={isNewPost ? "新建文章" : "编辑文章"}
            actions={
              <div style={{ display: "flex", gap: 8 }}>
                <ActionButton icon={Check} label="发布" busy={saveMutation.isPending} onClick={() => saveWithDraftState(false)} />
                <ActionButton icon={Save} label="存草稿" busy={saveMutation.isPending} onClick={() => saveWithDraftState(true)} />
              </div>
            }
          />
          <MutationError mutation={saveMutation} />
          <PostEditor
            draft={draft} isNewPost={isNewPost}
            saveMutation={saveMutation} isLoadingDetail={detailQuery.isLoading}
            categories={taxonomyQuery.data?.categories ?? []}
            draftCategory={draftCategory} draftCategoryTags={draftCategoryTags} selectedTags={selectedTags}
            onSave={saveWithDraftState} onUpdate={updateDraft}
            onSelectCategory={selectDraftCategory} onToggleTag={toggleTagInDraft}
          />
        </section>
      )}

      {/* ── 主页配置 ─────────────────────────────────────────────────────── */}
      <section className="section-band">
        <SectionHeader
          icon={Home}
          title="主页"
          actions={
            <ActionButton icon={Save} label="保存主页" busy={saveHomeMutation.isPending}
              onClick={() => saveHomeMutation.mutate(toBlogHomeSaveRequest(homeDraft))} />
          }
        />
        <BlogHomePanel
          draft={homeDraft} onUpdate={updateHomeDraft}
          isLoading={homeQuery.isLoading}
          loadError={homeQuery.error instanceof Error ? homeQuery.error : null}
          saveError={saveHomeMutation.error instanceof Error ? saveHomeMutation.error : null}
          isSaved={saveHomeMutation.isSuccess}
        />
      </section>

      {/* ── 已发布 ────────────────────────────────────────────────────────── */}
      <section className="section-band">
        <SectionHeader
          icon={Check}
          title={`已发布 · ${publishedCount} 篇`}
          actions={
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <BlogPostFilterToolbar
                query={query} categoryFilter={categoryFilter} featuredOnly={featuredOnly}
                categories={taxonomyQuery.data?.categories ?? []}
                onQueryChange={setQuery} onCategoryChange={setCategoryFilter} onFeaturedOnlyChange={setFeaturedOnly}
              />
              <ActionButton icon={Plus} label="新建文章" onClick={startNewPost} />
            </div>
          }
        />
        {postsQuery.isLoading ? <LoadingBlock label="正在读取文章" /> : null}
        <MutationError mutation={unpublishMutation} />
        <MutationError mutation={deleteMutation} />
        <div className="content-grid blog-post-list">
          {publishedPosts.map((post) => (
            <article
              className={selectedPath === post.relative_path ? "content-card blog-post-list-item active" : "content-card blog-post-list-item"}
              key={post.relative_path}
            >
              <CardInner>
                <CardRow label="标题">
                  <TruncatedText>
                    <TickerText>{post.title}</TickerText>
                  </TruncatedText>
                </CardRow>
                <CardRow label="分类">
                  <span className="blog-post-list-meta">
                    <TickerText>{post.category ?? "未分类"}</TickerText>
                  </span>
                </CardRow>
                <CardRow label="标签">
                  <span className="blog-post-list-tags">
                    {post.tags.slice(0, 3).map((tag) => <em key={tag}>{tag}</em>)}
                  </span>
                </CardRow>
                <CardActions>
                    <button type="button" className="card-action-button primary"
                      onClick={() => { setIsNewPost(false); setSelectedPath(post.relative_path); }}>
                      <FileText size={12} /><span>编辑</span>
                    </button>
                    <button type="button" className="card-action-button"
                      disabled={post.draft || unpublishMutation.isPending}
                      onClick={() => unpublishPost(post.relative_path)}>
                      <Square size={12} /><span>下线</span>
                    </button>
                    <button type="button" className="card-action-button danger"
                      disabled={deleteMutation.isPending}
                      onClick={() => deletePost(post.relative_path)}>
                      <Trash2 size={12} /><span>删除</span>
                    </button>
                </CardActions>
              </CardInner>
            </article>
          ))}
          {!postsQuery.isLoading && publishedPosts.length === 0 && (
            <div className="empty-state"><CircleHelp size={20} /><span>没有已发布的文章</span></div>
          )}
        </div>
      </section>

      {/* ── 草稿 ──────────────────────────────────────────────────────────── */}
      <section className="section-band">
        <SectionHeader icon={Square} title={`草稿 · ${draftCount} 篇`} />
        <div className="content-grid blog-post-list">
          {draftPosts.map((post) => (
            <article
              className={selectedPath === post.relative_path ? "content-card blog-post-list-item active" : "content-card blog-post-list-item"}
              key={post.relative_path}
            >
              <CardInner>
                <CardRow label="标题">
                  <TruncatedText>
                    <TickerText>{post.title}</TickerText>
                  </TruncatedText>
                </CardRow>
                <CardRow label="分类">
                  <span className="blog-post-list-meta">
                    <TickerText>{post.category ?? "未分类"}</TickerText>
                  </span>
                </CardRow>
                <CardRow label="标签">
                  <span className="blog-post-list-tags">
                    {post.tags.slice(0, 3).map((tag) => <em key={tag}>{tag}</em>)}
                  </span>
                </CardRow>
                <CardActions>
                    <button type="button" className="card-action-button primary"
                      onClick={() => { setIsNewPost(false); setSelectedPath(post.relative_path); }}>
                      <FileText size={12} /><span>编辑</span>
                    </button>
                    <button type="button" className="card-action-button danger"
                      disabled={deleteMutation.isPending}
                      onClick={() => deletePost(post.relative_path)}>
                      <Trash2 size={12} /><span>删除</span>
                    </button>
                </CardActions>
              </CardInner>
            </article>
          ))}
          {!postsQuery.isLoading && draftPosts.length === 0 && (
            <div className="empty-state"><CircleHelp size={20} /><span>暂无草稿</span></div>
          )}
        </div>
      </section>

      {/* ── 分类 ──────────────────────────────────────────────────────────── */}
      <section className="section-band">
        <SectionHeader
          icon={TagsIcon}
          title="分类"
          actions={
            <ActionButton
              icon={Plus}
              label="新建分类"
              busy={addCategoryMutation.isPending}
              onClick={() => addCategoryMutation.mutate("newclass")}
            />
          }
        />
        <TaxonomyPanel
          isLoading={taxonomyQuery.isLoading}
          featuredCount={featuredCount}
          categoryBlocks={categoryBlocks}
          addTagMutation={addTagMutation}
          renameTagMutation={renameTagMutation}
          deleteTagMutation={deleteTagMutation}
          renameCategoryMutation={renameCategoryMutation}
          deleteCategoryMutation={deleteCategoryMutation}
        />
      </section>

    </section>
  );
}
