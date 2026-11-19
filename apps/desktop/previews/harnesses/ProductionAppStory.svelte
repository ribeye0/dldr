<script lang="ts">
  import { emptyPlayerState } from "$lib/controllers/player-controller.svelte";
  import type { PlayerControl } from "$lib/api/player";
  import { onMount } from "svelte";
  import { mockIPC, clearMocks } from "@tauri-apps/api/mocks";
  import { emit } from "@tauri-apps/api/event";
  import App from "../../src/routes/+page.svelte";
  import { makeFixture } from "../fixtures/workspace";
  import {
    makeDetail,
    emptyFilters,
    queryProducts,
    facetsFor,
    type PreviewProduct,
  } from "../fixtures/library";
  import { buildLanguagePriority, defaultLanguagePriority, sameLanguagePriority } from "$lib/utils/name-language-priority";
  import { isActiveJob } from "$lib/utils/jobs";
  import type {
    DownloadQueueEntry,
    ProductListRequest,
    SaveAccountRequest,
    StartWorkDownloadRequest,
    BulkWorkDownloadRequest,
  } from "$lib/api/tauri";
  import type {
    Account,
    AppSettings,
    Product,
    ProductTextValue,
    JobSnapshot,
  } from "$lib/model/types";
  let ready = $state(false);
  onMount(() => {
    const fixture = makeFixture("long");
    const localFiles = new URLSearchParams(location.search).has("localFiles");
    const exportedWorks = new Set<string>();
    const setup = new URLSearchParams(location.search).has("setup");
    const setupAccountsKey = "dldr-story-setup-accounts";
    const timers: ReturnType<typeof setTimeout>[] = [];
    let products: Product[] = Array.from({ length: 3 }, (_, batch) =>
      fixture.products.map((p, i) => ({
        ...p,
        workId: `RJ${15000000 + batch * 72 + i}`,
        title: batch ? `${p.title} · Collection ${batch + 1}` : p.title,
      })),
    ).flat();
    if (localFiles) products = products.map(product => ({ ...product, download: {
      ...product.download, status: "downloaded", localPath: `/tmp/dlsite-library/${product.workId}`,
    } }));
    // Opt-in regression fixture: Play omitted credits, public metadata has two actors.
    const missingCredits = new URLSearchParams(location.search).has("missingCredits");
    if (missingCredits) {
      products[8] = {
        ...products[8], workId: "RJ01729125", workType: "SOU", creditGroups: [],
        latestPurchasedAt: "2026-12-31T00:00:00Z",
        tags: products[8].tags?.filter((tag) => tag.class === "genre"),
      };
    }
    const reliability = new URLSearchParams(location.search).has("reliability");
    const queueKey = "dlsite-story-reliability-queue";
    const globalLimitKey = "dlsite-story-global-limit";
    let globalLimit = Number(sessionStorage.getItem(globalLimitKey) ?? 2);
    let queue: DownloadQueueEntry[] = reliability ? JSON.parse(sessionStorage.getItem(queueKey) ?? "null") ?? products.slice(0,4).map((p,i) => ({
      workId:p.workId,title:p.title,thumbnailUrl:p.thumbnailUrl,batchId:"fixture",position:i,accountId:null,
      libraryRoot:"/tmp/dlsite-library",downloadRoot:"/tmp/dlsite-downloads",unpackPolicy:"unpack_when_recognized",replaceExisting:false,
      status:["running","paused","failed","completed"][i],errorMessage:i===2 ? "Synthetic connection failure" : null,
    })) : [];
    if (sessionStorage.getItem(queueKey) && reliability) queue = queue.map(e => ["running","queued","pausing","cancelling"].includes(e.status) ? {...e,status:"paused"} : e);
    const saveQueue = () => sessionStorage.setItem(queueKey,JSON.stringify(queue));
    if (reliability) saveQueue();
    let accounts: Account[] = setup ? JSON.parse(sessionStorage.getItem(setupAccountsKey) ?? "[]") : fixture.accounts.map((a) => ({ ...a }));
    const saveSetupAccounts = () => { if (setup) sessionStorage.setItem(setupAccountsKey, JSON.stringify(accounts)); };
    const catalog = products;
    if (setup && !accounts.some(account => account.lastSyncAt)) products = [];
    let jobs = (setup || localFiles ? [] : fixture.jobs).map((j) => ({
      ...j,
      title:
        j.kind === "accountSync" ? `Sync ${j.metadata.accountId}` : j.title,
    }));
    const settingsKey = setup ? "dldr-story-setup-settings" : "dldr-production-story-settings";
    const rateKey = "dldr-production-story-rate-limit";
    let downloadRateLimit: number | null = JSON.parse(sessionStorage.getItem(rateKey) ?? "null");
    const playlistKey = "dldr-production-story-playlists";
    let playlists: import("$lib/model/types").Playlist[] = JSON.parse(sessionStorage.getItem(playlistKey) ?? "null") ?? [{id:"0",name:"",workIds:[]}];
    let settings: AppSettings = {
      autoExtract: true,
      uiLocale: missingCredits ? "zh-TW" : "en",
      libraryRoot: setup ? null : "/tmp/dlsite-library",
      downloadRoot: setup ? null : "/tmp/dlsite-downloads",
      languagePriority: defaultLanguagePriority(missingCredits ? "zh-TW" : "en"),
      titleLanguageFollowsUi: true,
    };
    const storedSettings = sessionStorage.getItem(settingsKey);
    if (storedSettings) {
      const saved = JSON.parse(storedSettings);
      settings = { ...settings, ...saved };
      settings.titleLanguageFollowsUi = saved.titleLanguageFollowsUi ?? sameLanguagePriority(
        buildLanguagePriority(settings.languagePriority, settings.uiLocale), defaultLanguagePriority(settings.uiLocale ?? "en"));
    }
    settings.languagePriority = settings.titleLanguageFollowsUi
      ? defaultLanguagePriority(settings.uiLocale ?? "en") : buildLanguagePriority(settings.languagePriority, settings.uiLocale);
    const makerNames: Record<string, string> = {
      "North Window Studio": "北窓スタジオ",
      "Quiet Coast": "静かな海岸",
      "Paper Lantern": "紙灯籠",
    };
    function preferred(values: ProductTextValue[], priority = settings.languagePriority) {
      for (const code of priority) {
        const value = values.find(
          (entry) => entry.language === code && entry.value.trim(),
        );
        if (value) return value.value;
      }
      return [...values]
        .sort((a, b) =>
          a.language < b.language ? -1 : a.language > b.language ? 1 : 0,
        )
        .find((entry) => entry.value.trim())?.value;
    }
    function project(product: Product): PreviewProduct {
      const local = product.owners.every(
        (owner) => owner.accountId === "__local__",
      );
      const titleVariants = local
        ? [{ language: "und", value: product.title }]
        : [
            { language: "en_US", value: product.title },
            { language: "ja_JP", value: `静かな夜の物語 — ${product.workId}` },
            {
              language: "ko_KR",
              value: `조용한 밤의 이야기 — ${product.workId}`,
            },
            { language: "zh_CN", value: `静夜故事 — ${product.workId}` },
            { language: "zh_TW", value: `靜夜故事 — ${product.workId}` },
            {
              language: "pt_BR",
              value: `Uma noite tranquila — ${product.workId}`,
            },
            {
              language: "xx_XX",
              value: `Unknown code title — ${product.workId}`,
            },
          ];
      const names = product.makerName
        ? [
            { language: "en_US", value: product.makerName },
            {
              language: "ja_JP",
              value:
                makerNames[product.makerName] ?? `制作元 ${product.makerName}`,
            },
          ]
        : [];
      return {
        ...product,
        title: preferred(titleVariants) ?? product.workId,
        makerName: preferred(names, ["ja_JP", "zh_TW", "zh_CN", "en_US", "ko_KR"]) ?? null,
        makerKey: product.makerName ? `name:${product.makerName}` : undefined,
        titleVariants,
        makerNames: names,
      };
    }
    function filters(request: Omit<ProductListRequest, "limit" | "offset">) {
      return {
        genreNames: request.genreNames ?? [], excludedGenreNames: request.excludedGenreNames ?? [],
        voiceNames: request.voiceNames ?? [], excludedVoiceNames: request.excludedVoiceNames ?? [],
        downloadedOnly: request.downloadedOnly,
        ...emptyFilters(),
        accounts: request.accountIds,
        types: request.typeGroups,
        ages: request.ageCategories,
        sources: request.sourceGroups,
        makers: request.makerKeys,
        sort: request.sort as ReturnType<typeof emptyFilters>["sort"],
      };
    }
    let playerState = emptyPlayerState();
    const playerTracks = [
      {id:"WAV/01. はじまり.wav",name:"01. はじまり.wav",group:"WAV",format:"WAV",size:32*1024**2},
      {id:"WAV/02. おやすみ.wav",name:"02. おやすみ.wav",group:"WAV",format:"WAV",size:64*1024**2},
      {id:"MP3/01. はじまり.mp3",name:"01. はじまり.mp3",group:"MP3",format:"MP3",size:8*1024**2},
    ];
    mockIPC(
      async (command, payload) => {
        const args = payload as Record<string, unknown>;
        if (command === "report_frontend_failure") return;
        if (command === "list_playlists") return structuredClone(playlists);
        if (command === "edit_playlist") {
          const request = args.request as import("$lib/api/tauri").PlaylistEdit;
          let playlistId: string;
          if (request.action === "create") {
            if (!request.name.trim() || [...request.name.trim()].length>80 || /[\x00-\x1f\x7f]/.test(request.name)) throw new Error("Use a playlist name of 1–80 characters without control characters.");
            playlistId=crypto.randomUUID();playlists.push({id:playlistId,name:request.name.trim(),workIds:[]});
          } else {
            playlistId=request.playlistId;
            const list=playlists.find(list=>list.id===playlistId);
            if (!list) throw new Error("Playlist not found.");
            if (request.action === "rename") {if(!request.name.trim())throw new Error("Use a playlist name of 1–80 characters without control characters.");list.name=request.name.trim();}
            else if(request.action === "add") {
              if(!products.some(p=>p.workId===request.workId))throw new Error("Work not found.");
              if(!list.workIds.includes(request.workId))list.workIds.push(request.workId);
            } else {list.workIds=list.workIds.filter(id=>id!==request.workId);}
          }
          sessionStorage.setItem(playlistKey,JSON.stringify(playlists));
          return structuredClone({playlists,playlistId});
        }
        if (command === "set_native_header_theme") return;
        if (command === "export_work_download") {
          const id = String(args.workId);
          await new Promise(resolve => setTimeout(resolve, 80));
          if (exportedWorks.has(id)) throw new Error("Destination folder already exists");
          exportedWorks.add(id);
          return `${args.destinationRoot}/${id}`;
        }
        if (command === "delete_work_download") {
          const { workId } = args.request as { workId: string };
          const product = products.find(product => product.workId === workId);
          if (!product) throw new Error("Work not found");
          await new Promise(resolve => setTimeout(resolve, 80));
          product.download = { ...product.download, status: "notDownloaded", localPath: null, stagingPath: null };
          return structuredClone(product.download);
        }
        if (command === "player_snapshot") return structuredClone(playerState);
        if (command === "open_player") {
          const request = args.request as {workId:string};
          if (playerState.workId !== request.workId) playerState = {...emptyPlayerState(),workId:request.workId,revision:playerState.revision+1,durations:Object.fromEntries(playerTracks.map(track=>[track.id,120]))};
          return {media:{workId:request.workId,tracks:playerTracks,subtitles:[{id:"subtitles/01.vtt",name:"subtitles/01.vtt"},{id:"subtitles/02.vtt",name:"subtitles/02.vtt"}],hasArchives:false},state:structuredClone(playerState)};
        }
        if (command === "control_player") {
          const {workId,control} = args.request as {workId:string;control:PlayerControl};
          if(workId!==playerState.workId) throw new Error("The player control is invalid or outdated.");
          const s=playerState;
          switch(control.action) {
            case "play":s.trackId=control.trackId;s.paused=false;s.duration=120;s.subtitleId="subtitles/01.vtt";break;
            case "pause":s.paused=control.paused;break;
            case "seek":s.position=control.position;break;
            case "volume":s.volume=control.volume;break;
            case "speed":s.speed=control.speed;break;
            case "subtitle":s.subtitleId=control.subtitleId;break;
            case "offset":s.subtitleOffset=control.offset;break;
            case "stop":s.trackId=null;s.paused=true;s.position=0;s.duration=0;s.subtitleId=null;break;
            case "next":{const index=playerTracks.findIndex(t=>t.id===s.trackId);const next=playerTracks[index+control.direction];if(next?.group===playerTracks[index]?.group){s.trackId=next.id;s.position=0;s.subtitleId="subtitles/02.vtt";}break;}
          }
          s.revision++;return structuredClone(s);
        }
        if(command === "player_subtitle") return [
          {start:1,end:4,text:"歡迎回來。\nおかえりなさい。"},
          {start:3,end:6,text:"耳を澄ませて — Listen closely."},
          {start:8,end:12,text:"<b>字幕以文字安全顯示</b>"},
        ];
        if (command === "plugin:app|name") return "DLDR";
        if (command === "plugin:app|version") return "0.1.0-rc.1";
        if (command === "plugin:app|tauri_version") return "2.11.5";
        if (command === "plugin:app|identifier")
          return "io.github.ribeye0.dldr";
        if (command === "plugin:path|resolve_directory")
          return "/tmp/Downloads";
        if (command === "get_transfer_settings") return globalLimit;
        if (command === "set_transfer_concurrency") { globalLimit=Number(args.limit);sessionStorage.setItem(globalLimitKey,String(globalLimit));return; }
        if (command === "list_download_queue") return queue;
        if (command === "clear_download_queue") {queue=queue.filter(e => !["completed","cancelled"].includes(e.status));saveQueue();return;}
        if (command === "retry_failed_downloads") {const failed=queue.filter(e => e.status==="failed");failed.forEach(e => {e.status="queued";e.errorMessage=null;});saveQueue();return failed.length;}
        if (command === "control_download_queue") {
          if(args.action==="resumeAll") {queue.filter(e=>e.status==="paused").forEach(e=>e.status="queued");saveQueue();return;}
          const entry=queue.find(e=>e.workId===args.workId);if(!entry) throw new Error("Queue entry not found");
          if(args.action==="resume") {entry.status="queued";entry.errorMessage=null;saveQueue();return;}
          entry.status=args.action==="pause" ? "pausing" : "cancelling";saveQueue();
          setTimeout(()=>{entry.status=args.action==="pause" ? "paused" : "cancelled";saveQueue();},250);return;
        }
        if (command === "check_download_space") return {budgets:[{path:settings.libraryRoot,availableBytes:500*1024**3,requiredBytes:5*1024**3,reservedBytes:0,sufficient:true}],unknownCount:1};
        if (command === "verify_library") return {checked:12,present:11,missing:1,issues:[{workId:"RJ15000008",path:"/tmp/dlsite-library/RJ15000008",status:"missing"}]};
        if (command === "export_library_backup") return 42;
        if (command === "restore_library_backup") { queue=queue.map(e=>({...e,status:"paused"}));saveQueue();return 42; }
        if (command === "plugin:dialog|save") return reliability ? "/tmp/story-library-backup.json" : null;
        if (command === "get_settings") return settings;
        if (command === "get_setup_folders") return {libraryRoot:"/tmp/Downloads/dldr", downloadRoot:"/tmp/Downloads/dldr/.staging"};
        if (command === "configure_download_folders") {
          if (args.libraryRoot === "/not-writable") throw new Error("Permission denied (simulated)");
          settings = {...settings, libraryRoot: String(args.libraryRoot), downloadRoot: String(args.downloadRoot || `${args.libraryRoot}/.staging`), uiLocale: args.uiLocale as AppSettings["uiLocale"]};
          sessionStorage.setItem(settingsKey, JSON.stringify(settings));
          return settings;
        }
        if (command === "get_download_rate_limit") return { bytesPerSecond: downloadRateLimit };
        if (command === "set_download_rate_limit") {
          downloadRateLimit = (args.request as { bytesPerSecond: number | null }).bytesPerSecond;
          sessionStorage.setItem(rateKey, JSON.stringify(downloadRateLimit));
          return { bytesPerSecond: downloadRateLimit };
        }
        if (command === "list_name_languages")
          return [
            "en_US",
            "ja_JP",
            "ko_KR",
            "pt_BR",
            "und",
            "xx_XX",
            "zh_CN",
            "zh_TW",
          ];
        if (command === "save_settings") {
          const patch = args.settings as Partial<AppSettings>;
          if (patch.libraryRoot === "/fail-save")
            throw new Error("Could not save settings (simulated)");
          const followsUi = patch.titleLanguageFollowsUi ?? (patch.languagePriority ? false : settings.titleLanguageFollowsUi);
          settings = { ...settings, ...patch, titleLanguageFollowsUi: followsUi };
          settings.languagePriority = followsUi ? defaultLanguagePriority(settings.uiLocale ?? "en")
            : buildLanguagePriority(settings.languagePriority, settings.uiLocale);
          sessionStorage.setItem(settingsKey, JSON.stringify(settings));
          return settings;
        }
        if (command === "list_accounts") return accounts;
        if (
          command === "list_products" ||
          command === "list_product_filter_facets"
        ) {
          const request = args.request as ProductListRequest;
          const scoped = request.playlistId === undefined ? products : products.filter(p=>playlists.find(list=>list.id===request.playlistId)?.workIds.includes(p.workId));
          if (command === "list_product_filter_facets")
            return facetsFor(
              scoped.map(project),
              request.search ?? "",
              filters(request),
            );
          const matched = queryProducts(
            scoped.map(project),
            request.search ?? "",
            filters(request),
          );
          return {
            products: matched.slice(
              request.offset,
              request.offset + request.limit,
            ),
            totalCount: matched.length,
          };
        }
        if (command === "get_product_detail") {
          const product = products.find(
            (p) => p.workId === (args.request as { workId: string }).workId,
          );
          if (!product) throw new Error("Work not found");
          if (missingCredits && product.workId === "RJ01729125" && !product.creditGroups.length) {
            return new Promise((resolve) => setTimeout(() => {
              product.creditGroups = [{ kind: "voice", label: "Voice", names: ["伊倉える", "浅木式"] }];
              resolve(makeDetail(project(product)));
            }, 1200));
          }
          return makeDetail(project(product));
        }
        if (command === "get_product_page_content") {
          return '<section><h3>作品紹介 / Introduction</h3><p>Original product description.<br>Track 01 · Track 02</p><img src="data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 width=%22600%22 height=%22200%22%3E%3Crect width=%22600%22 height=%22200%22 fill=%22%232255aa%22/%3E%3C/svg%3E" alt="Product introduction"><a href="https://www.dlsite.com/">DLsite</a></section>';
        }
        if (command === "save_account") {
          const request = args.request as SaveAccountRequest;
          if (request.label === "fail")
            throw new Error("Could not save account (simulated)");
          const previous = accounts.find((a) => a.id === request.id);
          const account = {
            ...(previous ?? fixture.accounts[0]),
            ...(!previous && setup ? {lastLoginAt:null,lastSyncAt:null,enabled:true} : {}),
            id: request.id ?? crypto.randomUUID(),
            label: request.label,
            loginName: request.loginName,
            hasCredential:
              Boolean(request.password) || Boolean(previous?.hasCredential),
          } satisfies Account;
          accounts = [...accounts.filter((a) => a.id !== account.id), account];
          saveSetupAccounts();
          return account;
        }
        if (command === "set_account_enabled") {
          const request = args.request as {
            accountId: string;
            enabled: boolean;
          };
          accounts = accounts.map((a) =>
            a.id === request.accountId ? { ...a, enabled: request.enabled } : a,
          );
          return;
        }
        if (command === "remove_account") {
          const request = args.request as { accountId: string };
          accounts = accounts.filter((a) => a.id !== request.accountId);
          return {
            accountId: request.accountId,
            label: "Removed account",
            credentialDeleted: false,
          };
        }
        if (command === "list_jobs") return jobs;
        if (command === "cancel_job") {
          const job = jobs.find(
            (j) => j.id === (args.request as { jobId: string }).jobId,
          )!;
          job.status = "cancelling";
          return { outcome: "requested", snapshot: job };
        }
        if (command === "clear_finished_jobs") {
          const count = jobs.length;
          jobs = jobs.filter(isActiveJob);
          return { removedCount: count - jobs.length };
        }
        if (
          command === "start_work_download" ||
          command === "start_account_sync"
        ) {
          const request = args.request as StartWorkDownloadRequest;
          const job: JobSnapshot = {
            ...fixture.jobs[0],
            id: crypto.randomUUID(),
            status: "queued",
            phase: null,
            progress: null,
            kind:
              command === "start_account_sync" ? "accountSync" : "workDownload",
            metadata: { workId: request.workId, accountId: request.accountId },
          };
          jobs.push(job);
          if (setup && command === "start_account_sync") {
            const fail = new URLSearchParams(location.search).has("setupSyncError") && jobs.filter(j => j.kind === "accountSync").length === 1;
            timers.push(setTimeout(() => {
              job.status = fail ? "failed" : "succeeded";
              job.error = fail ? {code:"login_failed",message:"Test sign-in failed",details:{}} : null;
              if (!fail) {
                accounts = accounts.map(account => account.id === request.accountId ? {...account,lastSyncAt:new Date().toISOString()} : account);
                products = catalog;
                saveSetupAccounts();
              }
              void emit("dldr-job-event", {sequence:1,eventKind:"updated",jobId:job.id,kind:job.kind,status:job.status,phase:job.phase,progress:job.progress,message:null,log:null,snapshot:job});
            }, 700));
          }
          return { jobId: job.id };
        }
        if (command === "preview_bulk_work_download") {
          const bulk = args.request as BulkWorkDownloadRequest;
          const matches = queryProducts(products.map(project), bulk.search ?? "", filters(bulk));
          const selected = bulk.workIds ? matches.filter((p) => bulk.workIds!.includes(p.workId)) : matches;
          const skippedDownloadedCount = selected.filter((p) => p.download.status === "downloaded").length;
          const requestedCount = selected.length - skippedDownloadedCount;
          return {
            requestedCount, plannedCount: requestedCount, skippedDownloadedCount,
            skippedQueuedCount: 0, failedCount: 0, totalCount: selected.length,
            knownExpectedBytes: requestedCount * 1024 * 1024,
            totalExpectedBytes: requestedCount * 1024 * 1024, unknownSizeCount: 0,
          };
        }
        if (command === "start_bulk_work_download")
          return { jobId: "bulk-preview" };
        if (command === "list_audit_events") return fixture.events;
        if (command === "get_audit_log_dir")
          return { path: "/tmp/dlsite-logs" };
        if (command === "diagnostic_summary")
          return {
            runId: "run-story",
            health: {
              state: "available",
              droppedCritical: 0,
              droppedRoutine: 0,
              ringEvicted: 0,
            },
          };
        if (command === "diagnostic_runs")
          return [{ runId: "run-story", current: true }];
        if (command === "diagnostic_operation")
          return fixture.events.filter(
            (e) => e.operationId === args.operationId,
          );
        if (command === "export_diagnostics")
          return "/tmp/dlsite-diagnostics.zip";
        if (command === "save_product_cover") return true;
        if (command === "plugin:dialog|open") return localFiles ? "/tmp/export 日本語" : setup ? "/tmp/中文 日本語 Downloads/dldr" : reliability ? "/tmp/story-library-backup.json" : null;
        if (command === "check_app_update") return { kind:"appImage", release:null };
        if (
          [
            "open_dlsite_play",
            "open_external_url",
            "open_download_folder",
            "open_work_download",
            "open_audit_log_dir",
            "submit_two_factor_code",
            "cancel_two_factor",
          ].includes(command)
        )
          return;
        throw new Error(`Unimplemented Storybook command: ${command}`);
      },
      { shouldMockEvents: true },
    );
    ready = true;
    return () => { timers.forEach(clearTimeout); clearMocks(); };
  });
</script>

{#if ready}<App />{/if}
