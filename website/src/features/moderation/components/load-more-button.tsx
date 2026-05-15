import { Show } from "solid-js";
import { common } from "@/shared/i18n/generated";
import { t } from "@/shared/i18n/i18n";
import { Button } from "@/shared/ui/button";

interface LoadMoreButtonProps {
  hasMore: boolean;
  loading: boolean;
  onClick: () => void;
}

export function LoadMoreButton(props: LoadMoreButtonProps) {
  return (
    <Show when={props.hasMore}>
      <div class="flex justify-center py-3">
        <Button variant="outline" disabled={props.loading} onClick={props.onClick}>
          {props.loading ? t(common.loading) : t(common.loadMore)}
        </Button>
      </div>
    </Show>
  );
}
