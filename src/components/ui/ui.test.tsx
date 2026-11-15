import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

describe("shadcn components", () => {
  it("renders Button with variant data and handles clicks", () => {
    const onClick = vi.fn();
    render(
      <Button variant="outline" onClick={onClick}>
        확인
      </Button>,
    );
    const button = screen.getByRole("button", { name: "확인" });
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledOnce();
    expect(button.dataset.slot).toBe("button");
  });

  it("renders Card composition", () => {
    render(
      <Card>
        <CardHeader>
          <CardTitle>제목</CardTitle>
        </CardHeader>
        <CardContent>본문</CardContent>
      </Card>,
    );
    expect(screen.getByText("제목").dataset.slot).toBe("card-title");
    expect(screen.getByText("본문").closest("[data-slot=card]")).not.toBeNull();
  });
});
