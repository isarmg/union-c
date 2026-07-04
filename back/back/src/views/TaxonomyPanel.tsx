// 博客分类与标签管理面板（交互内联版）。
//
// 交互方式：
//   - 双击分类名 → 内联重命名输入框（Enter 确认，Escape 取消）
//   - 双击"标签"标签 → 在该分类下新建标签 "new-page"（之后可双击重命名）
//   - 双击标签文字 → 内联重命名输入框
//   - 分类卡片右上角 × → 删除分类
//   - 标签文字后 × → 删除标签

import { useEffect, useRef, useState } from "react";
import { Trash2 } from "lucide-react";
import { CardInner, CardRow, LoadingBlock, MutationError, TruncatedText } from "../components/ui";

type TagItem = { name: string; count: number };
type CategoryBlock = { name: string; count: number; tags: TagItem[] };
type Mut<V> = { mutate: (v: V) => void; isPending: boolean; isError: boolean; error: Error | null };

interface TaxonomyPanelProps {
  isLoading: boolean;
  featuredCount: number;
  categoryBlocks: CategoryBlock[];
  addTagMutation: Mut<{ name: string; category: string }>;
  renameTagMutation: Mut<{ from: string; to: string; category: string }>;
  deleteTagMutation: Mut<{ tag: string; category: string }>;
  renameCategoryMutation: Mut<{ from: string; to: string }>;
  deleteCategoryMutation: Mut<string>;
}

export function TaxonomyPanel({
  isLoading,
  featuredCount,
  categoryBlocks,
  addTagMutation,
  renameTagMutation,
  deleteTagMutation,
  renameCategoryMutation,
  deleteCategoryMutation,
}: TaxonomyPanelProps) {
  // 当前正在重命名的分类（存原始名）
  const [editCat, setEditCat] = useState<string | null>(null);
  const [editCatVal, setEditCatVal] = useState("");
  // 当前正在重命名的标签
  const [editTag, setEditTag] = useState<{ category: string; name: string } | null>(null);
  const [editTagVal, setEditTagVal] = useState("");
  // 各分类标签区是否已溢出（超出第2-6行）
  const tagListRefs = useRef<Map<string, HTMLElement | null>>(new Map());
  const [overflowed, setOverflowed] = useState<Set<string>>(new Set());

  // 每次标签数量变化后重新检测溢出
  useEffect(() => {
    const next = new Set<string>();
    for (const [cat, el] of tagListRefs.current.entries()) {
      if (!el) continue;
      const parent = el.parentElement;
      if (parent && el.scrollHeight > parent.clientHeight + 2) {
        next.add(cat);
      }
    }
    setOverflowed(next);
  }, [categoryBlocks]);

  // ── 分类重命名 ──────────────────────────────────────────────────────────────

  const startEditCat = (name: string) => {
    setEditCat(name);
    setEditCatVal(name);
    setEditTag(null);
  };

  const commitCatRename = (oldName: string) => {
    const newName = editCatVal.trim();
    setEditCat(null);
    if (newName && newName !== oldName) {
      renameCategoryMutation.mutate({ from: oldName, to: newName });
    }
  };

  // ── 标签重命名 ──────────────────────────────────────────────────────────────

  const startEditTag = (category: string, name: string) => {
    setEditTag({ category, name });
    setEditTagVal(name);
    setEditCat(null);
  };

  const commitTagRename = (category: string, oldName: string) => {
    const newName = editTagVal.trim();
    setEditTag(null);
    if (newName && newName !== oldName) {
      renameTagMutation.mutate({ from: oldName, to: newName, category });
    }
  };

  // ── 新建标签（双击"标签"标签触发）──────────────────────────────────────────

  const addDefaultTag = (category: string) => {
    if (overflowed.has(category)) return; // 标签区已满，禁止添加
    addTagMutation.mutate({ name: "new-page", category });
  };

  return (
    <>
      {isLoading ? <LoadingBlock label="正在读取标签" /> : null}
      <MutationError mutation={addTagMutation} />
      <MutationError mutation={renameTagMutation} />
      <MutationError mutation={deleteTagMutation} />
      <MutationError mutation={renameCategoryMutation} />
      <MutationError mutation={deleteCategoryMutation} />

      <div className="content-grid blog-category-block-grid">
        {/* 精选（系统分类，不可编辑） */}
        <div className="content-card blog-category-block system">
          <CardInner>
            <CardRow label="分类"><strong>精选</strong></CardRow>
            <CardRow label="文章">
              <TruncatedText muted>{featuredCount} 篇</TruncatedText>
            </CardRow>
            <CardRow label="标签" span={3}>
              <span className="muted-inline">系统分类，无标签</span>
            </CardRow>
          </CardInner>
        </div>

        {/* 用户自定义分类 */}
        {categoryBlocks.map((c) => {
          const isCatEditing = editCat === c.name;

          return (
            <div key={c.name} className="content-card blog-category-block">
              <CardInner>
                {/* 分类名（双击重命名） + 删除按钮 */}
                <CardRow label="分类">
                  {isCatEditing ? (
                    <input
                      autoFocus
                      className="taxonomy-inline-input"
                      value={editCatVal}
                      onChange={e => setEditCatVal(e.target.value)}
                      onKeyDown={e => {
                        if (e.key === "Enter") commitCatRename(c.name);
                        if (e.key === "Escape") setEditCat(null);
                      }}
                      onBlur={() => commitCatRename(c.name)}
                    />
                  ) : (
                    <TruncatedText
                      className="taxonomy-editable"
                      onDoubleClick={() => startEditCat(c.name)}
                      title="双击重命名"
                    >
                      {c.name}
                    </TruncatedText>
                  )}
                  <button
                    className="taxonomy-delete-btn"
                    title={`删除分类 ${c.name}`}
                    onClick={() => {
                      if (window.confirm(`确定删除分类"${c.name}"？此操作将清空所有文章的此分类。`))
                        deleteCategoryMutation.mutate(c.name);
                    }}
                  >
                    <Trash2 size={11} />
                  </button>
                </CardRow>

                {/* 标签区（第2-6行）：label 双击=新建标签，各标签双击=重命名，× =删除 */}
                <CardRow
                  label={
                    <span
                      className={`taxonomy-label-clickable${overflowed.has(c.name) ? " taxonomy-label-full" : ""}`}
                      onDoubleClick={() => addDefaultTag(c.name)}
                      title={overflowed.has(c.name) ? "标签已满，无法继续添加" : "双击添加标签"}
                    >
                      标签
                    </span>
                  }
                  span={5}
                >
                  {c.tags.length ? (
                    <span
                      className="taxonomy-tag-list"
                      ref={(el) => {
                        tagListRefs.current.set(c.name, el);
                      }}
                    >
                      {c.tags.map(t => {
                        const isTagEditing =
                          editTag?.category === c.name && editTag?.name === t.name;
                        return isTagEditing ? (
                          <input
                            key={t.name}
                            autoFocus
                            className="taxonomy-inline-input taxonomy-tag-input"
                            value={editTagVal}
                            onChange={e => setEditTagVal(e.target.value)}
                            onKeyDown={e => {
                              if (e.key === "Enter") commitTagRename(c.name, t.name);
                              if (e.key === "Escape") setEditTag(null);
                            }}
                            onBlur={() => commitTagRename(c.name, t.name)}
                          />
                        ) : (
                          <span key={t.name} className="taxonomy-tag-item">
                            <span
                              className="taxonomy-editable"
                              onDoubleClick={() => startEditTag(c.name, t.name)}
                              title="双击重命名"
                            >
                              {t.name}
                            </span>
                            <button
                              className="taxonomy-tag-delete"
                              title={`删除标签 ${t.name}`}
                              onClick={() => deleteTagMutation.mutate({ tag: t.name, category: c.name })}
                            >
                              ×
                            </button>
                          </span>
                        );
                      })}
                    </span>
                  ) : (
                    <span className="muted-inline">双击"标签"新建</span>
                  )}
                </CardRow>
              </CardInner>
            </div>
          );
        })}
      </div>
    </>
  );
}
