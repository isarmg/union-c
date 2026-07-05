// 博客文章编辑表单面板。
// 只负责渲染和调用上层传入的回调，不持有任何自身状态。

import { Star } from "lucide-react";
import { LoadingBlock, MutationError } from "../components/ui";
import type { BlogDraft } from "./blog-types";
import { DEFAULT_BLOG_IMAGE_DISPLAY_PATH, slugify } from "./blog-types";

type TagItem = { name: string; count: number };
type CategoryItem = { name: string; count: number };

// UseMutationResult 的最小公共接口，避免引入完整泛型
type MutationState = { isPending: boolean; isError: boolean; error: Error | null };

interface PostEditorProps {
  draft: BlogDraft;
  isNewPost: boolean;
  saveMutation: MutationState;
  isLoadingDetail: boolean;
  categories: CategoryItem[];
  draftCategory: string;
  draftCategoryTags: TagItem[];
  selectedTags: string[];
  onSave: (asDraft: boolean) => void;
  onUpdate: (patch: Partial<BlogDraft>) => void;
  onSelectCategory: (category: string) => void;
  onToggleTag: (tag: string) => void;
}

export function PostEditor({
  draft,
  isNewPost,
  saveMutation,
  isLoadingDetail,
  categories,
  draftCategory,
  draftCategoryTags,
  selectedTags,
  onSave,
  onUpdate,
  onSelectCategory,
  onToggleTag
}: PostEditorProps) {
  return (
    <>
      {isLoadingDetail ? <LoadingBlock label="正在读取文章详情" /> : null}
      <MutationError mutation={saveMutation} />

      <div className="blog-editor-form">
        <label className="inline-field">
          <span>标题</span>
          <input
            value={draft.title}
            onChange={(e) => {
              const title = e.target.value;
              onUpdate({
                title,
                relative_path: isNewPost && !draft.pathTouched
                  ? `${slugify(title)}.md`
                  : draft.relative_path
              });
            }}
            placeholder="文章标题"
          />
        </label>
        <label className="inline-field">
          <span>路径</span>
          <input
            value={draft.relative_path}
            onChange={(e) => onUpdate({ relative_path: e.target.value, pathTouched: true })}
            placeholder="my-post.md"
          />
        </label>
        <label className="inline-field wide">
          <span>摘要</span>
          <textarea rows={2} value={draft.description} onChange={(e) => onUpdate({ description: e.target.value })} placeholder="用于首页、列表和 SEO 描述" />
        </label>
        <div className="blog-editor-meta-row wide">
          <label className="inline-field">
            <span>发布日期</span>
            <input type="date" value={draft.pub_date} onChange={(e) => onUpdate({ pub_date: e.target.value })} />
          </label>
          <label className="inline-field">
            <span>更新日期</span>
            <input type="date" value={draft.updated_date} onChange={(e) => onUpdate({ updated_date: e.target.value })} />
          </label>
          <label className="inline-field">
            <span>作者</span>
            <input value={draft.author} onChange={(e) => onUpdate({ author: e.target.value })} placeholder="Local Control" />
          </label>
          <label className="inline-field">
            <span>封面图</span>
            <input value={draft.hero_image} onChange={(e) => onUpdate({ hero_image: e.target.value })} placeholder={DEFAULT_BLOG_IMAGE_DISPLAY_PATH} />
          </label>
        </div>

        {/* 分类选择 */}
        <div className="taxonomy-inline-palette wide">
          <button
            type="button"
            className={draft.featured ? "tag-chip special active" : "tag-chip special"}
            onClick={() => onUpdate({ featured: !draft.featured })}
            title="切换首页精选"
          >
            <Star size={13} />首页精选
          </button>
          <button type="button" className={!draft.category ? "tag-chip active" : "tag-chip"} onClick={() => onSelectCategory("")}>未分类</button>
          {categories.map((c) => (
            <button
              key={c.name}
              type="button"
              className={draft.category === c.name ? "tag-chip active" : "tag-chip"}
              onClick={() => onSelectCategory(c.name)}
            >
              {c.name} <span>{c.count}</span>
            </button>
          ))}
        </div>

        {/* 标签选择 */}
        <div className="taxonomy-inline-palette wide">
          {!draftCategory ? (
            <span className="muted-inline">先选择普通分类，才能选择标签</span>
          ) : draftCategoryTags.length ? (
            draftCategoryTags.map((tag) => (
              <button
                key={tag.name}
                type="button"
                className={selectedTags.includes(tag.name) ? "tag-chip selected" : "tag-chip"}
                onClick={() => onToggleTag(tag.name)}
              >
                {tag.name} <span>{tag.count}</span>
              </button>
            ))
          ) : (
            <span className="muted-inline">当前分类暂无标签，请到分类板块添加</span>
          )}
        </div>

        <label className="inline-field content-field">
          <span>正文 Markdown / MDX</span>
          <textarea value={draft.content} onChange={(e) => onUpdate({ content: e.target.value })} placeholder="## 小标题&#10;&#10;正文内容..." />
        </label>
      </div>
    </>
  );
}
