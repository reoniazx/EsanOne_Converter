import { describe, expect, it } from "vitest";
import { normalizeLocale, loadStoredLocale } from "./locale";
import { t } from "./messages";

describe("i18n", () => {
  it("normalizes locale tags", () => {
    expect(normalizeLocale("th-TH")).toBe("th");
    expect(normalizeLocale("en-US")).toBe("en");
    expect(normalizeLocale("fr")).toBe("th");
  });

  it("translates Thai keys", () => {
    expect(t("th", "EsanOne Converter")).toBe("EsanOne Converter");
    expect(t("th", "Separate audio")).toBe("ค้นหา");
    expect(t("th", "Download song")).toBe("ดาวน์โหลดเพลง");
    expect(t("th", "Stem model missing notice")).toContain(
      "ตอนนี้คุณยังไม่มี Model แยกเสียง"
    );
    expect(t("th", "GPU faster notice")).toContain("GPU");
    expect(t("th", "Minimize window")).toBe("ย่อหน้าต่าง");
    expect(t("th", "Close window")).toBe("ปิดหน้าต่าง");
    expect(t("th", "Separate")).toBe("แยกเสียง");
    expect(t("th", "Separate new")).toBe("แยกใหม่");
    expect(t("th", "Waveform")).toBe("รูปคลื่นเสียง");
    expect(t("th", "Cover art")).toBe("รูปปกเพลง");
    expect(t("th", "History")).toBe("ประวัติ");
    expect(t("th", "No history yet")).toBe("ยังไม่มีประวัติเพลง");
    expect(t("th", "Separates")).toBe("แยกเป็น");
    expect(t("th", "Advantages")).toBe("ข้อดี");
    expect(t("th", "Limitations")).toBe("ข้อจำกัด");
    expect(t("th", "Vocals and instrumental")).toBe("เสียงร้อง และดนตรีประกอบ");
    expect(t("th", "Vocals (MelBandRoFormer)")).toBe("เสียงร้อง (MelBandRoFormer)");
    expect(t("th", "Instruments 4 (HT-Demucs)")).toBe("เครื่องดนตรี 4 (HT-Demucs)");
    expect(t("th", "Instruments 6 (HT-Demucs 6s)")).toBe("เครื่องดนตรี 6 (HT-Demucs 6s)");
    expect(t("th", "Dochord chords")).toBe("คอร์ดเพลงจาก Dochord");
    expect(t("th", "Search Dochord on Google")).toBe("ค้นหาเพลงใน Google");
    expect(t("th", "Searching for chords…")).toBe("กำลังค้นหาคอร์ดเพลง…");
    expect(t("en", "Vocals and instrumental")).toBe("Vocals and instrumental");
    expect(t("en", "Separate")).toBe("Separate");
    expect(t("th", "GPU setup could not be completed")).toContain("GPU");
    expect(t("en", "Analysis could not be completed")).toBe(
      "Analysis could not be completed"
    );
  });

  it("loads a stored or default locale", () => {
    localStorage.removeItem("esanone_stem_locale");
    expect(loadStoredLocale()).toBe("th");
    localStorage.setItem("esanone_stem_locale", "en");
    expect(loadStoredLocale()).toBe("en");
    localStorage.removeItem("esanone_stem_locale");
  });
});
