import { useState, useEffect, forwardRef, useImperativeHandle } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { SkillCard } from "./SkillCard";
import { RepoManagerPanel } from "./RepoManagerPanel";
import { skillsApi, type Skill, type SkillRepo } from "@/lib/api/skills";

interface SkillsPageProps {
  onClose?: () => void;
}

export interface SkillsPageHandle {
  refresh: () => void;
  openRepoManager: () => void;
}

export const SkillsPage = forwardRef<SkillsPageHandle, SkillsPageProps>(
  ({ onClose: _onClose }, ref) => {
    const { t } = useTranslation();
    const [skills, setSkills] = useState<Skill[]>([]);
    const [repos, setRepos] = useState<SkillRepo[]>([]);
    const [loading, setLoading] = useState(true);
    const [repoManagerOpen, setRepoManagerOpen] = useState(false);

    const loadSkills = async (afterLoad?: (data: Skill[]) => void) => {
      try {
        setLoading(true);
        const data = await skillsApi.getAll();
        setSkills(data);
        if (afterLoad) {
          afterLoad(data);
        }
      } catch (error) {
        toast.error(t("skills.loadFailed"), {
          description:
            error instanceof Error ? error.message : t("common.error"),
        });
      } finally {
        setLoading(false);
      }
    };

    const loadRepos = async () => {
      try {
        const data = await skillsApi.getRepos();
        setRepos(data);
      } catch (error) {
        console.error("Failed to load repos:", error);
      }
    };

    useEffect(() => {
      Promise.all([loadSkills(), loadRepos()]);
    }, []);

    useImperativeHandle(ref, () => ({
      refresh: () => loadSkills(),
      openRepoManager: () => setRepoManagerOpen(true),
    }));

    const handleInstall = async (directory: string) => {
      try {
        await skillsApi.install(directory);
        toast.success(t("skills.installSuccess", { name: directory }));
        await loadSkills();
      } catch (error) {
        toast.error(t("skills.installFailed"), {
          description:
            error instanceof Error ? error.message : t("common.error"),
        });
      }
    };

    const handleUninstall = async (directory: string) => {
      try {
        await skillsApi.uninstall(directory);
        toast.success(t("skills.uninstallSuccess", { name: directory }));
        await loadSkills();
      } catch (error) {
        toast.error(t("skills.uninstallFailed"), {
          description:
            error instanceof Error ? error.message : t("common.error"),
        });
      }
    };

    const handleAddRepo = async (repo: SkillRepo) => {
      await skillsApi.addRepo(repo);

      let repoSkillCount = 0;
      await Promise.all([
        loadRepos(),
        loadSkills((data) => {
          repoSkillCount = data.filter(
            (skill) =>
              skill.repoOwner === repo.owner &&
              skill.repoName === repo.name &&
              (skill.repoBranch || "main") === (repo.branch || "main"),
          ).length;
        }),
      ]);

      toast.success(
        t("skills.repo.addSuccess", {
          owner: repo.owner,
          name: repo.name,
          count: repoSkillCount,
        }),
      );
    };

    const handleRemoveRepo = async (owner: string, name: string) => {
      await skillsApi.removeRepo(owner, name);
      toast.success(t("skills.repo.removeSuccess", { owner, name }));
      await Promise.all([loadRepos(), loadSkills()]);
    };

    return (
      <div className="flex flex-col h-full min-h-0 bg-background/50">
        {/* 顶部操作栏（固定区域）已移除，由 App.tsx 接管 */}

        {/* 技能网格（可滚动详情区域） */}
        <div className="flex-1 min-h-0 overflow-y-auto px-6 py-4 animate-fade-in">
          {loading ? (
            <div className="flex items-center justify-center h-64">
              <RefreshCw className="h-8 w-8 animate-spin text-muted-foreground" />
            </div>
          ) : skills.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-64 text-center">
              <p className="text-lg font-medium text-gray-900 dark:text-gray-100">
                {t("skills.empty")}
              </p>
              <p className="mt-2 text-sm text-gray-500 dark:text-gray-400">
                {t("skills.emptyDescription")}
              </p>
              <Button
                variant="link"
                onClick={() => setRepoManagerOpen(true)}
                className="mt-3 text-sm font-normal"
              >
                {t("skills.addRepo")}
              </Button>
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
              {skills.map((skill) => (
                <SkillCard
                  key={skill.key}
                  skill={skill}
                  onInstall={handleInstall}
                  onUninstall={handleUninstall}
                />
              ))}
            </div>
          )}
        </div>

        {/* 仓库管理面板 */}
        {repoManagerOpen && (
          <RepoManagerPanel
            repos={repos}
            skills={skills}
            onAdd={handleAddRepo}
            onRemove={handleRemoveRepo}
            onClose={() => setRepoManagerOpen(false)}
          />
        )}
      </div>
    );
  },
);

SkillsPage.displayName = "SkillsPage";
